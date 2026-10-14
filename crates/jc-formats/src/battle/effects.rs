
use crate::action::{Point, WINDOWS};
use crate::battle::Spell;
use crate::battle::record::Draws;
use crate::light::Movable;
use crate::package::{Kind, Package, Whom};
use crate::skeleton::{BoneScale, sine};

/// The loader's table in the executable: four bytes an effect, the package file and the
/// game frames its windows are moved by.
pub const FILES: u32 = 0x8007_9514;
/// The effects of the table.
pub const COUNT: usize = 70;
/// The effect the loader takes for a number of [`COUNT`] or more.
pub const OTHER: usize = 18;
pub const CAPTURE: usize = 37;
pub const SUMMON: usize = 57;
pub const LEAVE: usize = 58;

pub const ACTOR_ATTACK: usize = 19;
pub const ACTOR_SUMMON: usize = 58;
pub const ACTOR_LEAVE: usize = 57;
/// The effect of a spell whose number is past the table (`0x8001AC8C`).
pub const NO_SPELL: usize = 26;

/// The effect a spell shows (`0x8001AC8C(spell)`): the byte at +6 of its record.
pub fn of_spell(spells: &[Spell], spell: usize) -> usize {
    (spells.get(spell)).map_or(NO_SPELL, |spell| usize::from(spell.shown))
}

pub fn of_body_attack(slot: u32, attack: u8) -> Option<usize> {
    let by_slot = match slot {
        0..4 => 38 + super::moves::body_attack(slot),
        _ => 30,
    };
    Some(match attack {
        16 => 38,
        17 => 60,
        18 => 59,
        21 => 30,
        24 => 34,
        28 => return None,
        33 => 25,
        34..=37 => 29 + usize::from(attack),
        38 => 52,
        _ => by_slot,
    })
}

pub fn half_of_body_attack(attack: u8) -> i16 {
    match attack {
        2 | 3 => i16::from(attack),
        4 => 0,
        5 | 23 | 25 | 29 => 1,
        6 | 20 => 4,
        7 | 22 => 5,
        8 | 16 => 6,
        9 => 10,
        10 => 8,
        11 | 19 => 9,
        34..=37 => i16::from(attack) - 34,
        _ => -1,
    }
}

/// The sprite routine 23 plays for the byte of the half word of a move (the jump
/// table `0x80072134` of the byte plus one): none for a byte outside -1 to 9.
pub fn sprite_of_half(half: u8) -> Option<u32> {
    Some(match (half as i8).checked_add(1)? {
        0 | 8..=10 => 35,
        1 => 20,
        2 => 8,
        3 => 17,
        4 => 34,
        5 => 26,
        6 => 28,
        7 => 32,
        _ => return None,
    })
}

/// What routine 24 shows for a byte of the half word of a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The sprite of its thirty pieces; none for the byte that only plays the model.
    pub sprite: Option<u32>,
    /// The record of the light's colour, of the five at `0x80079500`.
    pub light: usize,
    /// The pieces are drawn to the actor in place of rising as a fountain.
    pub drawn_in: bool,
    /// The model [`thing::OF_ROUTINE_24`] is played once at the point.
    pub model: bool,
}

/// Routine 24 by the byte of the half word of a move (the jump table `0x80072164` of
/// the byte plus one): nothing for a byte outside -1 to 10.
pub fn hit_of_half(half: u8) -> Option<Hit> {
    let (sprite, light, drawn_in, model) = match (half as i8).checked_add(1)? {
        0 => (35, 0, false, false),
        1 => (20, 1, false, false),
        2 => (8, 4, false, false),
        3 => (17, 3, false, false),
        4 => (34, 2, false, false),
        5 => (26, 4, false, false),
        6 => (28, 4, false, false),
        7 => (32, 4, false, false),
        8 => (14, 4, false, false),
        9 => (35, 4, true, false),
        10 => (35, 2, true, true),
        11 => {
            return Some(Hit {
                sprite: None,
                light: 0,
                drawn_in: false,
                model: true,
            });
        }
        _ => return None,
    };
    Some(Hit {
        sprite: Some(sprite),
        light,
        drawn_in,
        model,
    })
}

pub fn of_swap(kind: u32, creature_in_front: bool) -> Option<usize> {
    match (kind, creature_in_front) {
        (21, false) | (22, true) => Some(ACTOR_LEAVE),
        (21, true) | (22, false) => Some(ACTOR_SUMMON),
        _ => None,
    }
}

/// The words of the effect of the player's attack by the value of his weapon
/// (`0x800189F8`), and the bytes of the effect of an item (`0x8001CE34`).
pub const ATTACKS: u32 = 0x8007_962C;
pub const ITEMS: u32 = 0x8007_CB48;
/// The items of the table, and the effect of an item past it.
pub const ITEM_COUNT: usize = 43;
pub const NO_ITEM: usize = 34;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbers {
    /// The value of each weapon: the half word at +2 of its record.
    pub weapon_values: Vec<i16>,
    /// The effect by the value of the weapon.
    pub attacks: Vec<i32>,
    /// The effect of each item.
    pub items: Vec<u8>,
}

impl Numbers {
    /// From the executable as it is on the disc, whose first byte is at RAM address
    /// `base`.
    pub fn read(image: &[u8], base: u32) -> Option<Self> {
        use crate::model::{WEAPON_ITEMS, WEAPON_RECORD, WEAPON_RECORDS, WEAPONS};
        let at = |address: u32| Some(address.checked_sub(base)? as usize);
        let records = at(WEAPON_RECORDS)?;
        let weapon_values = (0..WEAPON_ITEMS)
            .map(|weapon| {
                let value = records + weapon * WEAPON_RECORD + 2;
                let bytes = image.get(value..value + 2)?;
                Some(i16::from_le_bytes([bytes[0], bytes[1]]))
            })
            .collect::<Option<_>>()?;
        let attacks = at(ATTACKS)?;
        let (words, _) = image.get(attacks..attacks + 4 * WEAPONS)?.as_chunks::<4>();
        let items = at(ITEMS)?;
        Some(Self {
            weapon_values,
            attacks: words.iter().map(|word| i32::from_le_bytes(*word)).collect(),
            items: image.get(items..items + ITEM_COUNT)?.to_vec(),
        })
    }

    /// The effect of the player's attack with the weapon he has (`0x800189F8()`): the
    /// word of the weapon's value. None without a weapon of the table, where the
    /// console reads the value from memory that is not on the disc, and for a value
    /// or a word past the tables.
    pub fn of_attack(&self, weapon: u8) -> Option<usize> {
        let value = *self.weapon_values.get(usize::from(weapon))?;
        let word = *self.attacks.get(usize::try_from(value).ok()?)?;
        usize::try_from(word).ok()
    }

    /// The effect of an item (`0x8001CE34(item)`): its byte, [`NO_ITEM`] past the table.
    pub fn of_item(&self, item: u32) -> usize {
        (self.items.get(item as usize)).map_or(NO_ITEM, |&number| usize::from(number))
    }
}

pub fn weapon_half_word(weapon: u8) -> Option<i16> {
    Some(match weapon {
        2 => 3,
        7 => 8,
        9 => 1,
        11 => 4,
        13 => 5,
        14 => 0,
        18 => 2,
        21 => 7,
        _ => return None,
    })
}

pub fn of_request(kind: u32, parameter: u32, spells: &[Spell]) -> Option<usize> {
    match kind {
        0 => Some(ACTOR_ATTACK),
        1 => Some(of_spell(spells, parameter as usize)),
        2 => Some(ACTOR_SUMMON),
        3 => Some(ACTOR_LEAVE),
        _ => None,
    }
}

/// The bit of a slot's byte that routine 1 tests: one of the two the resolver sets for
/// an action that missed or did nothing (`resolve::effect::MISSED` is both).
pub const MISSED: u8 = 0x02;
/// The bit of the same byte of a party the action missed or whose status was refused.
pub const REFUSED: u8 = 0x04;
/// The bits of the same byte of a party that lost a layer of its shield, and the sound
/// routine 51 plays for it.
pub const SHIELD_WENT: u8 = 0x38;
pub const SHIELD_SOUND: u32 = 73;

/// The nodes of the pool of the effect tasks, and of the pool of the small tasks.
pub const TASKS: usize = 20;
pub const SMALL_TASKS: usize = 150;

/// One frame of a sprite a game frame.
pub const SPRITE_SPEED: i32 = 4096;
/// The blend word a small task hands the quad drawer: blended, the texel added.
pub const SPRITE_BLEND: u16 = 48;
/// The flag word of the shell of a capture and of the broken shell: every face blended,
/// the texel added, the colours mixed towards black by the share in the upper half.
pub const SHELL_FLAGS: u32 = 0xB0;
/// The flag word the pieces of routine 6 are handed to the mesh drawer with: blended
/// in mode 1, not lit, not faded.
pub const PIECE_FLAGS: u32 = 48;
/// The word the stitch quads of a model of an effect are drawn with (`0x80045A08` and
/// `0x80045E6C` from the model player `0x800C35B4` and the ribbon's routine
/// `0x800C3198`): blended in mode 1, whatever the words of the model's objects are.
pub const STITCH_WORD: u32 = 48;
/// Routine 6 (`0x800C2AE4`): the pieces that stand round the party, how far from it
/// they start, the count at which they start to close in and the count of their end,
/// at which they break; the turn of a piece in a game frame.
pub const PIECES: i32 = 8;
pub const PIECES_FAR: i32 = 300;
pub const PIECES_CLOSE: i32 = 26;
pub const PIECES_BREAK: i32 = 30;
pub const PIECE_TURN: i16 = 170;
/// The shards the pieces break into, and the sound of the bank that goes with it.
pub const SHARDS: i32 = 24;
pub const SHARD_SOUND: u32 = 79;
/// The clip of routine 6 is asked for this many game frames.
pub const PIECES_CLIP: i32 = 100;
/// The model of routine 32 (`0x800173A8`): what its size grows by in its first game
/// frame, less an eighth, 4096 being the model's own size; and the game frames its
/// case holds row 13 of the fight's sequences for.
pub const SWELL_GROWTH: i32 = 0x23_F000;
pub const SWELL_ROW_FRAMES: i32 = 53;
/// The model of routine 30 (`0x8001761C`): what it turns by a game frame, the count
/// from which it fades, by [`SHARE_STEP`] a game frame, and its last count.
pub const WOBBLE_TURN: i16 = 128;
pub const WOBBLE_FADES: i32 = 45;
pub const WOBBLE_LAST: i32 = 61;
/// The pieces of the two fountains of routine 30, the second only on a party that took
/// what was done to it, and the game frames of its clip.
pub const FOUNTAINS: [i32; 2] = [12, 30];
pub const FOUNTAIN_CLIP: i32 = 100;
/// What the sprite of a piece of a fountain turns by a game frame.
pub const FOUNTAIN_TURN: i16 = 64;
/// The lights of routine 20 (`0x800C2FF8`), a turn apart by their number, and the
/// rings of the ribbon each of them drags.
pub const FLYING: i32 = 6;
pub const RINGS: usize = 4;
/// The shell is drawn at this many halves of its target's scale.
/// The routines 41 to 43: every fourth frame of its path the light sheds three pieces,
/// a third of a turn apart, each 20 from the line it falls along, turning by 128 a game
/// frame and shrinking by 64 of 4096 until it is below a half.
pub const SHED_EVERY: usize = 4;
pub const SHED: i16 = 3;
pub const SHED_APART: i16 = 1365;
pub const SHED_OFF: [i16; 3] = [20, 0, 0];
pub const SHED_TURN: i16 = 128;
pub const SHED_SHRINKS: i32 = 64;
pub const SHED_SMALLEST: i32 = 2048;
pub const SHELL_HALVES: i32 = 3;
/// The speed of the sprite of routine 23: two frames a game frame.
pub const FAST_SPRITE: i32 = 8192;
/// Routine 24 (`0x80015AE8`): the row of the fight's sequences it starts, the pieces
/// of its fountain or of the sprites that are drawn in (`0x800C1824`), the actor's
/// bone those head for, and the game frames its light is on (`0x80017208`).
pub const ROW_OF_ROUTINE_24: u32 = 19;
pub const PIECES_OF_ROUTINE_24: i32 = 30;
pub const DRAWN_TO_BONE: usize = 2;
pub const LIGHT_OF_ROUTINE_24: i32 = 32;
/// The movable light of a fight (`0x800C4260`): how far each of its three callers lets
/// it reach, the five colour records of routine 24 (`0x80079500`), and the colours of
/// the shell of a capture (`0x800B6A18` of file 36) and of routine 32 (`0x80072194`),
/// as red, green and blue.
pub const LIGHT_RANGE: i32 = 1500;
pub const LIGHT_COLOURS: [[u8; 3]; 5] = [
    [214, 214, 255],
    [255, 0, 107],
    [21, 0, 255],
    [0, 255, 148],
    [235, 255, 0],
];
pub const SHELL_LIGHT: [u8; 3] = [232, 232, 128];
pub const SWELL_LIGHT: [u8; 3] = [255, 128, 0];
/// +68 of the memory of every node of the pool of the effect tasks, as far as the
/// build knows what was last written there: the place of a step's task, of a light and
/// of the model that swells. The shell of a capture reads it without having written it.
pub type Kept = [Option<[i32; 3]>; TASKS];
/// The sound of the routines 11, 15, 16 and 17.
pub const SOUND_OF_ROUTINE_17: u32 = 72;
/// The routines 19 and 44: every how many game frames their second model is played,
/// and their third and fourth, each with the count of its first time.
pub const SUMMON_EVERY: i32 = 12;
pub const SUMMON_OTHERS: (i32, [i32; 2]) = (9, [3, 6]);
/// The sound the routines 23 and 52 hold for the length of their window.
pub const WINDOW_SOUND: u32 = 69;
/// Routine 38: its three sounds. Routine 39: the sound it holds and for how many game
/// frames, the game frames its task counts, and the sound at their end.
pub const SOUNDS_OF_ROUTINE_38: [u32; 3] = [32, 33, 34];
pub const HELD_BY_ROUTINE_39: (i32, u32) = (20, 35);
pub const ROUTINE_39_COUNTS: i32 = 3;
pub const SOUND_AFTER_ROUTINE_39: u32 = 36;
/// Routine 40 (`0x80016E20`): the count of its start and of its end, its two sounds
/// at the start and the one at the end, and how many pieces stand round the place.
pub const ROUTINE_40_STARTS: i32 = 1;
pub const ROUTINE_40_ENDS: i32 = 93;
pub const SOUNDS_OF_ROUTINE_40: [u32; 2] = [24, 74];
pub const SOUND_AFTER_ROUTINE_40: u32 = 26;
pub const ROUND_ROUTINE_40: i32 = 5;
/// Routine 47 (`0x80016FC8`): the two sounds it holds, each with its game frames, the
/// counts of its second sprite and of its end, the pieces round the place and the
/// game frames of its clip.
pub const HELD_BY_ROUTINE_47: [(i32, u32); 2] = [(50, 30), (80, 29)];
pub const ROUTINE_47_SECOND: i32 = 48;
pub const ROUTINE_47_ENDS: i32 = 51;
pub const ROUND_ROUTINE_47: i16 = 4;
pub const ROUTINE_47_CLIP: i32 = 100;
/// Routine 27: the share its shape gains a game frame, and loses of its count while
/// it stands whole; its sound and the game frames of its clip.
pub const COVER_STEP: i32 = 128;
pub const COVER_SOUND: u32 = 74;
pub const COVER_CLIP: i32 = 64;
/// What the share of a shell moves by in a game frame, and its whole.
pub const SHARE_STEP: i32 = 256;
pub const SHARE_ONE: i32 = 4096;
/// The game frames a shell grows and stands at most, and the clip of the light plays.
pub const CAPTURE_FRAMES: i32 = 600;
/// The mode word the player of the package's clip hands the picture decoder
/// (`0x80017B94`): every pixel that comes out has the blend mark.
pub const CLIP_MODE: u32 = 2;
/// The light in the player's hand turns by this a game frame, and the angle of its
/// swell goes on by this.
pub const GLOW_TURN: i32 = 32;
pub const GLOW_SWELL: i32 = 64;

/// The player's bone the follow-up of a capture flies to: the bone of his point kind 20,
/// where he holds the cocoon.
pub const HAND_BONE: usize = 7;

/// What the asker of a capture leaves in the half word `0x8008D712` (`0x80013358`):
/// the grades 0 and 2 catch.
pub fn catches(grade: u32) -> bool {
    matches!(grade, 0 | 2)
}

/// The things of the capture's package the routines name by number.
pub mod thing {
    pub const TRAIL: u32 = 0;
    /// The hit of routine 1: a burst of the one and the other played once.
    pub const HIT_BURST: u32 = 1;
    pub const HIT: u32 = 2;
    /// The models the routines 7 and 31 play once, each in its own package.
    pub const OF_ROUTINE_7: u32 = 16;
    pub const OF_ROUTINE_31: u32 = 26;
    /// Routine 6: the model whose first mesh object closes in, the model whose first
    /// eight objects are the shards, and the sprite of the break.
    pub const PIECE: u32 = 24;
    pub const SHARD: u32 = 25;
    pub const BREAK: u32 = 33;
    /// Routine 8: its two sprites; the first gives the clip its count.
    pub const OF_ROUTINE_8: [u32; 2] = [10, 9];
    /// Routine 29: its two sprites, the first of which gives the clip its count,
    /// and its model.
    pub const OF_ROUTINE_29: [u32; 2] = [12, 11];
    pub const MODEL_OF_ROUTINE_29: u32 = 4;
    /// Routine 33: the sprite of its burst and its two models.
    pub const BURST_OF_ROUTINE_33: u32 = 19;
    /// Routine 32: the model that swells and the sprite played once.
    pub const SWELL: u32 = 17;
    pub const SPRITE_OF_ROUTINE_32: u32 = 23;
    /// Routine 30: the model of two shapes and the sprite of the fountains.
    pub const WOBBLE: u32 = 5;
    pub const FOUNTAIN: u32 = 13;
    /// Routine 20: the sprite of a light that flies in, the model of its ribbon and
    /// the model played once.
    pub const FLYING: u32 = 24;
    pub const RIBBON: u32 = 19;
    pub const OF_ROUTINE_20: u32 = 18;
    /// Routine 51: the model of a shield that went.
    pub const SHIELD: u32 = 6;
    /// The model of the routines 11, 15, 16 and 17, and the one of routine 12.
    pub const OF_ROUTINE_17: u32 = 28;
    pub const OF_ROUTINE_12: u32 = 177;
    /// The model routine 24 plays once for two bytes of the half word of a move.
    pub const OF_ROUTINE_24: u32 = 192;
    /// The routines 19 and 44: the model of their start, the one of every twelfth
    /// game frame and the two of every ninth.
    pub const OF_SUMMON: [u32; 4] = [33, 34, 35, 36];
    /// The sprite routine 21 plays every game frame, and the one of routine 52.
    pub const OF_ROUTINE_21: u32 = 39;
    pub const OF_ROUTINE_52: u32 = 428;
    /// Routine 38: its sprite and its two models. Routine 39: its sprite and model.
    pub const SPRITE_OF_ROUTINE_38: u32 = 7;
    pub const MODELS_OF_ROUTINE_38: [u32; 2] = [1, 2];
    pub const SPRITE_OF_ROUTINE_39: u32 = 25;
    pub const MODEL_OF_ROUTINE_39: u32 = 20;
    /// Routine 40: the sprite and the model that stand round the place, and the model
    /// of its end. Routine 47: the sprite and the model round the place, its second
    /// sprite and the model of its end.
    pub const ROUND_SPRITE_OF_ROUTINE_40: u32 = 374;
    pub const ROUND_MODEL_OF_ROUTINE_40: u32 = 178;
    pub const LAST_MODEL_OF_ROUTINE_40: u32 = 179;
    pub const ROUND_SPRITE_OF_ROUTINE_47: u32 = 22;
    pub const ROUND_MODEL_OF_ROUTINE_47: u32 = 15;
    pub const SECOND_SPRITE_OF_ROUTINE_47: u32 = 21;
    pub const LAST_MODEL_OF_ROUTINE_47: u32 = 14;
    /// The model of two shapes of routine 27.
    pub const COVER: u32 = 21;
    /// Routine 48: the sprite played once and the sprite of its fountain; routine 50:
    /// the sprite played once and the sprites of its two fountains.
    pub const OF_ROUTINE_48: [u32; 2] = [27, 26];
    pub const OF_ROUTINE_50: [u32; 3] = [30, 29, 28];
    /// The routines 41 to 43: the sprite whose path the light takes, and the sprites
    /// its pieces show, the first for an odd count of sheddings.
    pub const OF_ROUTINE_41: [u32; 3] = [4, 3, 3];
    pub const OF_ROUTINE_42: [u32; 3] = [6, 5, 5];
    pub const OF_ROUTINE_43: [u32; 3] = [4, 3, 5];
    /// The model of the routines 28 and 34 to 36, the model and the fountain's sprite
    /// of routine 49, and the fountain's sprite of routine 13.
    pub const OF_ROUTINE_28: u32 = 23;
    pub const OF_ROUTINE_49: u32 = 22;
    pub const FOUNTAIN_OF_ROUTINE_49: u32 = 32;
    pub const FOUNTAIN_OF_ROUTINE_13: u32 = 31;
    pub const OF_ROUTINE_33: [u32; 2] = [12, 13];
    /// Routine 9: the sprite and the model that stand five times round the point.
    pub const ROUND_SPRITE: u32 = 18;
    pub const ROUND_MODEL: u32 = 11;
    pub const CAST: u32 = 0;
    pub const CAST_OTHER: u32 = 3;
    pub const AROUND_SPRITE: u32 = 36;
    pub const AROUND_MODEL: u32 = 191;
    pub const SHELL: u32 = 30;
    pub const BROKEN: u32 = 31;
    pub const GLOW: u32 = 37;
    pub const FLARE: u32 = 38;
}

/// The routines of the step that are built, and those the step has no case for.
const BUILT: [u32; 46] = [
    0, 1, 3, 4, 6, 7, 8, 9, 11, 12, 13, 15, 16, 17, 18, 19, 20, 21, 23, 24, 25, 26, 27, 28, 29, 30,
    31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 47, 48, 49, 50, 51, 52,
];
const NO_CASE: [u32; 7] = [2, 5, 10, 14, 22, 45, 46];

/// One entry of the loader's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub file: usize,
    /// Added to every frame of the windows that is not negative.
    pub shift: i16,
}

/// The loader's table, from the executable as it is on the disc, whose first byte is at
/// RAM address `base`.
pub fn table(image: &[u8], base: u32) -> Option<Vec<Entry>> {
    let at = FILES.checked_sub(base)? as usize;
    let (entries, _) = image.get(at..at + 4 * COUNT)?.as_chunks::<4>();
    Some(
        entries
            .iter()
            .map(|entry| Entry {
                file: usize::from(u16::from_le_bytes([entry[0], entry[1]])),
                shift: i16::from_le_bytes([entry[2], entry[3]]),
            })
            .collect(),
    )
}

/// The entry the loader reads for a number.
pub fn entry(table: &[Entry], number: usize) -> Option<Entry> {
    let number = if number >= COUNT { OTHER } else { number };
    table.get(number).copied()
}

/// The windows as the reader of section 0 leaves them (`0x8001852C`): the effect's own
/// count of frames is added to every frame that is not negative.
pub fn shifted(mut windows: [[i16; 2]; WINDOWS], shift: i16) -> [[i16; 2]; WINDOWS] {
    for frame in windows.iter_mut().flatten() {
        if *frame >= 0 {
            *frame = frame.wrapping_add(shift);
        }
    }
    windows
}

/// A party of an effect, as the description names it: the points of its model, the
/// scales of its bones and its own scale, when it has one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Body {
    pub points: Vec<Point>,
    pub bone_scales: Vec<BoneScale>,
    pub scale: Option<[i32; 3]>,
    /// The byte of the party's slot in the loader's record (+68 and the slot; +52 of a
    /// target in the description): what the resolver left for it
    /// (`resolve::effect`). A task on a target keeps its target's at +172, a task on
    /// the acting party the first target's; the acting party's own is not read.
    pub byte: u8,
}

impl Body {
    /// The point of a kind (`0x80018908`): its bone and its place in the bone's space,
    /// which is the point list's times the bone's scale and times the party's scale. A
    /// record that only stands for another kind hands on to that kind's record.
    pub fn point(&self, kind: u16) -> Option<(usize, [i32; 3])> {
        let of = |kind: i16| self.points.iter().find(|point| point.kind == kind);
        let first = of(i16::try_from(kind).ok()?)?;
        let point = match first.stands_for() {
            Some(other) => of(other)?,
            None => first,
        };
        let bone = usize::try_from(point.bone).ok()?;
        let stretch =
            (self.bone_scales.get(bone)).map_or([SHARE_ONE; 3], |b| b.scale.map(i32::from));
        let place = std::array::from_fn(|axis| {
            let scaled = i32::from(point.place[axis]) * stretch[axis] / SHARE_ONE;
            match self.scale {
                Some(scale) => (i64::from(scaled) * i64::from(scale[axis]) / 4096) as i32,
                None => scaled,
            }
        });
        Some((bone, place))
    }
}

/// A party of the effect that runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Actor,
    /// A target by its place in the list the effect was started with.
    Target(usize),
}

/// What the tasks read of the fight in a game frame.
pub trait Field {
    /// The place in the world of a point of a bone: the point turned and moved by the
    /// bone's matrix, plus the bone's entry of the party's table, plus the party's place.
    fn point(&self, who: Who, bone: usize, place: [i32; 3]) -> [i32; 3];
    /// The bone's entry of the party's table plus the party's place.
    fn bone(&self, who: Who, bone: usize) -> [i32; 3];
    /// The party's place.
    fn stands(&self, who: Who) -> [i32; 3];
    /// The player's bone 7, where he holds the cocoon: its entry plus his place.
    fn hand(&self) -> [i32; 3];
    /// The height of the ground (`0x8008BCAC`); y grows downwards.
    fn ground(&self) -> i32;
    /// The library's arc tangent (`0x8005BC5C`, `mover::ArcTangent::angle`).
    fn arc(&self, across: i32, along: i32) -> i16;
    fn scale(&self, _who: Who, started: Option<[i32; 3]>) -> Option<[i32; 3]> {
        started
    }
}

/// What a game frame of the effects shows. Sprites and models are entries of the
/// package's sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drawn {
    /// A frame of a sprite at a place, blended by [`SPRITE_BLEND`].
    Sprite {
        sprite: usize,
        frame: usize,
        place: [i32; 3],
        angles: [i16; 3],
    },
    /// The placements of a frame of a model, turned about y and put at a place.
    Model {
        model: usize,
        frame: usize,
        place: [i32; 3],
        turn: i16,
    },
    /// The first mesh object of a model, mixed from its second (share 0) to its first
    /// ([`SHARE_ONE`]), with [`SHELL_FLAGS`] and the fade share [`SHARE_ONE`] less the
    /// share. `scale` is the target's own, of which the shell takes [`SHELL_HALVES`]
    /// halves ([`shell_scale`]); the broken shell has none.
    Shape {
        model: usize,
        share: i32,
        place: [i32; 3],
        scale: Option<[i32; 3]>,
        whole: bool,
    },
    /// A mesh object of a model at a place, turned by its angles and scaled alike on
    /// every axis, drawn with [`PIECE_FLAGS`].
    Piece {
        model: usize,
        object: usize,
        place: [i32; 3],
        angles: [i16; 3],
        scale: i32,
    },
    /// The first mesh object of a model, mixed from its second (share 0) to its first
    /// ([`SHARE_ONE`]) as a shape is, turned by its angles and scaled alike on every
    /// axis, drawn lit and blended (the flag word `0x31`) and faded by `fade` of
    /// [`SHARE_ONE`].
    Mixed {
        model: usize,
        share: i32,
        place: [i32; 3],
        angles: [i16; 3],
        scale: i32,
        fade: i32,
    },
    /// A frame of a sprite whose quads all stand at `offset` from the place, turned by
    /// the angles, in place of their own places; blended by [`SPRITE_BLEND`].
    Thrown {
        sprite: usize,
        frame: usize,
        place: [i32; 3],
        angles: [i16; 3],
        offset: [i16; 3],
    },
    /// A piece a light of the routines 41 to 43 has shed: the first quad of the first
    /// frame of a sprite, [`SHED_OFF`] from the place and turned by the angles, in
    /// place of its own place, with its sizes taken times `size` of [`SHARE_ONE`];
    /// blended by [`SPRITE_BLEND`].
    Shed {
        sprite: usize,
        place: [i32; 3],
        angles: [i16; 3],
        size: i32,
    },
    /// The ribbon of a light of routine 20: the first four mesh objects of a model,
    /// each at its own place with the same angles, drawn with [`PIECE_FLAGS`] and
    /// joined by the model's stitch quads, which are drawn with [`STITCH_WORD`].
    Ribbon {
        model: usize,
        points: [[i32; 3]; RINGS],
        angles: [i16; 3],
    },
    /// The light in the player's hand: the first quad of sprite `glow` in front of
    /// everything, in the `count`-th game frame of its routine, with the rings of the
    /// sprite `flare`.
    Glow {
        glow: usize,
        flare: Option<usize>,
        place: [i32; 3],
        count: i32,
    },
}

/// What a game frame of the effects asks of the sound and of the clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asks {
    /// `0x800314F0(number)`: a sound of a bank.
    Sound(u32),
    /// `0x800311BC(row, 1)`: a row of the fight's sequences.
    Row(u32),
    /// `0x80031390(row)`: the row is stopped and made silent.
    RowStop(u32),
    /// `0x8003013C(number, 1)`: way 1 of an animation event.
    Way(u32),
    /// `0x80030758(number)`.
    WayEnd(u32),
    /// `0x80031550(number)`: a sound of a bank whose voice the asker keeps, to stop
    /// it later.
    SoundHeld(u32),
    /// `0x800315D0(voice, number)`: a sound of a bank that was held is stopped.
    SoundStop(u32),
    /// `0x80017A6C(task, frames)`: the package's first clip, a picture a game frame.
    Clip(i32),
}

/// The memory of a task of the step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Task {
    who: Who,
    bone: usize,
    /// The point in the bone's space.
    point: [i32; 3],
    scale: Option<[i32; 3]>,
    /// +0, and +4, which each routine uses as it likes.
    counter: i32,
    own: i32,
    start: i16,
    end: i16,
    routine: u32,
    /// +172: the byte of the party's slot.
    byte: u8,
    /// +165: the low byte of the loader's half word of a move.
    half: u8,
    /// The place in the world of this game frame and of the one before.
    at: [i32; 3],
    before: [i32; 3],
}

/// A node of the pool of the effect tasks by its routine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    /// The step `0x80015478`.
    Step(Task),
    /// `0x80014C28`: a model at the target every tenth game frame.
    Rings(Task),
    /// `0x80014CCC`: the light in the hand.
    Glow(Task),
    /// `0x800C35B4`: a model played once.
    Model {
        model: usize,
        frame: usize,
        place: [i32; 3],
        turn: i16,
    },
    /// `0x800C3198`: a sprite played once whose last quad drags a ribbon of rings:
    /// the place of this game frame and three earlier ones.
    Flying {
        sprite: usize,
        model: Option<usize>,
        frame: usize,
        steps: i32,
        place: [i32; 3],
        turn: i16,
        points: [[i32; 3]; RINGS],
    },
    /// `0x800C1620`: a place that goes along the first quads of a sprite's frames and
    /// sheds pieces. `count` is +0 of the task, `counter` +4, 4096 to a frame.
    Shedding {
        path: usize,
        sprites: [Option<usize>; 2],
        count: i32,
        counter: i32,
        place: [i32; 3],
    },
    /// `0x8001761C`: a model of two shapes that swells, turns and fades.
    Wobble {
        model: usize,
        counter: i32,
        size: i32,
        growth: i32,
        turn: i16,
        place: [i32; 3],
    },
    /// `0x800173A8`: the first mesh object of a model that swells at a place for a
    /// count of game frames.
    Swell {
        model: usize,
        counter: i32,
        end: i32,
        size: i32,
        growth: i32,
        place: [i32; 3],
    },
    /// `0x80017E30`, then `0x80017FF8`: a model of two shapes that grows round a bone
    /// as the shell of a capture does, stands whole and is gone. `left` is what the
    /// share still lacks, `stays` the count of the second routine once it runs.
    Cover {
        model: usize,
        who: Who,
        bone: usize,
        scale: Option<[i32; 3]>,
        left: i32,
        stays: Option<i32>,
    },
    /// `0x80016BF4`: counts game frames down and ends with a sound.
    Ending { left: i32 },
    Light {
        record: usize,
        place: [i32; 3],
        counter: i32,
    },
    /// `0x80016E20` (routine 40) and `0x80016FC8` (routine 47): a count of game
    /// frames at which things are played at the task's place and at its party's,
    /// turned towards the actor.
    Timed {
        routine: u32,
        who: Who,
        place: [i32; 3],
        counter: i32,
    },
    /// `0x800C38EC`: the shell of a capture.
    Shell {
        who: Who,
        bone: usize,
        scale: Option<[i32; 3]>,
        share: i32,
        counter: i32,
    },
    /// `0x800C3FC8`: the spark of a creature that is caught.
    Spark { place: [i32; 3], speed: [i32; 3] },
    /// `0x800C3F04`: the broken shell of one that is not.
    Broken { place: [i32; 3], share: i32 },
    /// `0x80017B10`: the player of the package's first clip, which counts to `end`.
    Clip { counter: i32, end: i32 },
}

/// A small task: a sprite played once, which a burst also moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Small {
    sprite: usize,
    /// 4096 is one frame.
    counter: i32,
    speed: i32,
    place: [i32; 3],
    angles: [i16; 3],
    /// The speed of a piece of a burst, 1024 being one unit a game frame.
    moves: Option<[i32; 3]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Piece {
    /// `0x800C25A0`: counts to the break at the party's place.
    Gather { counter: i32, place: [i32; 3] },
    /// `0x800C2EA8`: a piece that stands round the place, turns and closes in.
    Closing {
        counter: i32,
        model: usize,
        origin: [i32; 3],
        offset: [i32; 3],
        speed: [i32; 3],
        angles: [i16; 3],
        scale: i32,
    },
    /// `0x800C2340`: a piece of a fountain, a sprite that rises round its origin.
    Rising {
        sprite: usize,
        /// 4096 is one frame of the sprite.
        counter: i32,
        speed: i32,
        origin: [i32; 3],
        /// In whole units.
        offset: [i32; 3],
        moves: [i32; 3],
        angles: [i16; 3],
    },
    /// `0x800C19BC`: a sprite that is drawn to the actor's bone [`DRAWN_TO_BONE`].
    /// The place is in whole units, the speed in 1024ths.
    Pulled {
        sprite: usize,
        /// 4096 is one frame of the sprite.
        counter: i32,
        speed: i32,
        place: [i32; 3],
        moves: [i32; 3],
    },
    /// `0x800C13F0`: a piece a light has shed, which falls the faster the larger it
    /// is, turns and shrinks. The place and the fall are in 1024ths of a unit.
    Falling {
        sprite: usize,
        size: i32,
        place: [i32; 3],
        fall: i32,
        turn: i16,
    },
    /// `0x800C290C`: a shard that flies off, falls and ends on the ground.
    Shard {
        model: usize,
        object: usize,
        origin: [i32; 3],
        offset: [i32; 3],
        speed: [i32; 3],
        angles: [i16; 3],
        spin: i16,
        scale: i32,
    },
}

/// The angles of a direction (`0x8003B5D8`): the tilt is less the arc tangent of y
/// over the length on the ground, the turn the arc tangent of x over z.
fn heading(field: &impl Field, way: [i32; 3]) -> [i16; 3] {
    let [x, _, z] = way.map(i64::from);
    let level = root(x * x + z * z);
    [
        field.arc(way[1], level).wrapping_neg(),
        field.arc(way[0], way[2]),
        0,
    ]
}

/// The effect tasks and the small tasks of a fight.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Effects {
    tasks: [Option<Node>; TASKS],
    small: Vec<Small>,
    /// The small tasks of routine 6.
    pieces: Vec<Piece>,
    drawn: Vec<Drawn>,
    asked: Vec<Asks>,
    /// The half word `0x8008D712`: the capture that runs catches its target.
    caught: bool,
    /// +60 of the loader's record: the game frames it still waits.
    waits: i32,
    /// The small tasks that hold a row of the fight's sequences (`0x800C1288`): the
    /// row and the count that is left.
    held: Vec<(u32, i32)>,
    /// The small tasks that hold a sound of a bank (`0x800C1360`): the sound and the
    /// count that is left.
    held_sounds: Vec<(u32, i32)>,
    /// The word `0x8008D714`: a task of this effect holds the sound of its window
    /// already.
    window_sound: bool,
    /// +64 of the loader's record: the half word the last move left (`0x80018430`).
    half: i16,
    unread: Vec<u32>,
    /// The movable light of a fight as the tasks have left it (`0x8008D610`,
    /// `0x8008D3A8`, `0x8008D380`); none while its range is 0.
    light: Option<Movable>,
    kept: Kept,
    /// Bit `0x800` of the engine's word `0x8008D608` with the picture it stands for:
    /// the picture of the clip that the last pass has unpacked.
    picture: Option<usize>,
}

/// The sum of the three squared differences of two places (`0x8003B810`).
fn squared_distance(a: [i32; 3], b: [i32; 3]) -> i64 {
    (0..3)
        .map(|axis| {
            let d = i64::from(a[axis]) - i64::from(b[axis]);
            d * d
        })
        .sum()
}

/// The whole root (`0x8003B91C`).
fn root(value: i64) -> i32 {
    value.max(0).isqrt() as i32
}

impl Effects {
    /// The tasks of an effect with this half word of a move in the loader's record.
    pub fn with_half(mut self, half: i16) -> Self {
        self.half = half;
        self
    }

    /// The tasks of an effect in a pool whose nodes hold what the effects before left
    /// in them.
    pub fn with_kept(mut self, kept: Kept) -> Self {
        self.kept = kept;
        self
    }

    /// What the nodes of the pool hold at +68 now.
    pub fn kept(&self) -> Kept {
        self.kept
    }

    /// The movable light of the fight as the last pass left it.
    pub fn light(&self) -> Option<Movable> {
        self.light
    }

    /// The movable light of a fight is set (`0x800C4260(place, colour, range, t)`): at
    /// the place, as far as [`LIGHT_RANGE`], each byte of the colour times t / 128, so
    /// that a t of 4096 gives 32 times the byte.
    fn shine(&mut self, place: [i32; 3], colour: [u8; 3], t: i32) {
        self.light = Some(Movable {
            place,
            colour: colour.map(|byte| (i32::from(byte) * t) >> 7),
            range: LIGHT_RANGE,
        });
    }

    /// The start of an effect (`0x800184DC` with the starter `0x800149A4`): record i of
    /// the package's first section starts one task at every point kind its party has,
    /// with pair i of `windows` as its start and its end, when that start is not -1. The
    /// windows are those of the acting fighter's action, moved by the loops
    /// (`player::arms::later_windows`) and by the effect's own count ([`shifted`]).
    /// `caught` is what the asker of a capture left for its end.
    pub fn start(
        &mut self,
        package: &Package,
        windows: [[i16; 2]; WINDOWS],
        actor: &Body,
        targets: &[Body],
        caught: bool,
    ) {
        self.caught = caught;
        self.window_sound = false;
        // A task on the acting party keeps the byte of the first target, not the
        // actor's own: the reader hands the starter the word at +120 of the
        // description for it, which is +52 of target 0 (`0x80018748`). Stand-in: 0
        // without a target, where the game reads what its stack held.
        let of_the_first = targets.first().map_or(0, |body| body.byte);
        for (record, [start, end]) in package.start_records().into_iter().zip(windows) {
            let Some(record) = record else { continue };
            let parties: Vec<(Who, &Body)> = match record.whom {
                Whom::First => vec![(Who::Actor, actor)],
                Whom::Each => (targets.iter().enumerate())
                    .map(|(index, body)| (Who::Target(index), body))
                    .collect(),
            };
            for (who, body) in parties {
                for &kind in &record.points {
                    let Some((bone, point)) = body.point(kind) else {
                        continue;
                    };
                    if start == -1 {
                        continue;
                    }
                    if !BUILT.contains(&record.number)
                        && !NO_CASE.contains(&record.number)
                        && !self.unread.contains(&record.number)
                    {
                        self.unread.push(record.number);
                    }
                    self.waits = self.waits.max(i32::from(end));
                    self.take(
                        Node::Step(Task {
                            who,
                            bone,
                            point,
                            scale: body.scale,
                            counter: 0,
                            own: 0,
                            start,
                            end,
                            routine: record.number,
                            byte: match who {
                                Who::Actor => of_the_first,
                                Who::Target(_) => body.byte,
                            },
                            half: self.half as u8,
                            at: [0; 3],
                            before: [0; 3],
                        }),
                        None,
                    );
                }
            }
        }
    }

    /// The first node of the pool that is not in use takes the task (`0x80043B1C`);
    /// `busy` is the node whose task is being stepped. A full pool takes nothing.
    fn take(&mut self, node: Node, busy: Option<usize>) {
        let free = (0..TASKS).find(|&index| Some(index) != busy && self.tasks[index].is_none());
        if let Some(index) = free {
            self.tasks[index] = Some(node);
        }
    }

    /// The clip's maker (`0x80017A6C(task, frames)`): an effect task that counts to
    /// `frames`, or to the count of the clip's pictures when `frames` is not above 0.
    /// A full pool takes none.
    fn clip(&mut self, package: &Package, frames: i32, busy: usize) {
        self.asked.push(Asks::Clip(frames));
        let end = match frames > 0 {
            true => frames,
            false => package.clips.first().map_or(0, |clip| clip.len() as i32),
        };
        self.take(Node::Clip { counter: 0, end }, Some(busy));
    }

    /// A sprite played once (`0x800C1C2C`), by its number in the package.
    fn sprite(&mut self, package: &Package, number: u32, place: [i32; 3], angles: [i16; 3]) {
        self.sprite_at(package, number, place, angles, SPRITE_SPEED);
    }

    /// A sprite played once at a speed, 4096 being a frame a game frame.
    fn sprite_at(
        &mut self,
        package: &Package,
        number: u32,
        place: [i32; 3],
        angles: [i16; 3],
        speed: i32,
    ) {
        let Some(sprite) = package.find(Kind::Sprite, number) else {
            return;
        };
        if self.small_free() {
            self.small.push(Small {
                sprite,
                counter: 0,
                speed,
                place,
                angles,
                moves: None,
            });
        }
    }

    /// A burst (`0x800C1E2C`): sixteen pieces at the place, each with three draws for
    /// its speed and a fourth for the speed of its sprite.
    fn burst(&mut self, package: &Package, number: u32, place: [i32; 3], draws: &mut impl Draws) {
        let Some(sprite) = package.find(Kind::Sprite, number) else {
            return;
        };
        for _ in 0..16 {
            let moves = [(); 3].map(|()| ((draws.draw() & 63) as i32 - 31) * 1024);
            let speed = 2048 + (draws.draw() & 4095) as i32 / 2;
            if self.small_free() {
                self.small.push(Small {
                    sprite,
                    counter: 0,
                    speed,
                    place,
                    angles: [0; 3],
                    moves: Some(moves),
                });
            }
        }
    }

    /// A model played once (`0x800C34AC`), by its number in the package. Answers the
    /// model's count of frames, as the game's maker does; 0 for a package without the
    /// model, where the game's answer is not read.
    fn model(
        &mut self,
        package: &Package,
        number: u32,
        place: [i32; 3],
        turn: i16,
        busy: usize,
    ) -> i32 {
        let Some(model) = package.find(Kind::Model, number) else {
            return 0;
        };
        self.take(
            Node::Model {
                model,
                frame: 0,
                place,
                turn,
            },
            Some(busy),
        );
        package.models[model].frames() as i32
    }

    /// A fountain (`0x800C21CC(place, angles, sprite, speed, count)`): `count` pieces
    /// at the place, each with a draw for its speed along x, one for its speed upwards,
    /// one for z and one for the speed of its sprite. A full pool takes no piece and
    /// makes no draw for it.
    fn fountain(
        &mut self,
        package: &Package,
        number: u32,
        place: [i32; 3],
        speed: i32,
        count: i32,
        draws: &mut impl Draws,
    ) {
        let Some(sprite) = package.find(Kind::Sprite, number) else {
            return;
        };
        for _ in 0..count {
            if !self.small_free() {
                continue;
            }
            let x = ((draws.draw() & 63) as i32 - 31) * 1024;
            let y = -((draws.draw() & 31) as i32) * 1024;
            let z = ((draws.draw() & 63) as i32 - 31) * 1024;
            let speed = speed + 2048 + (draws.draw() & 4095) as i32 / 2;
            self.pieces.push(Piece::Rising {
                sprite,
                counter: 0,
                speed,
                origin: place,
                offset: [0; 3],
                moves: [x, y, z],
                angles: [0; 3],
            });
        }
    }

    /// The pieces that are drawn in (`0x800C1824(place, angles, sprite, _, the actor's
    /// place, the entry of its bone)`): [`PIECES_OF_ROUTINE_24`] small tasks at the
    /// place, each with a draw for its speed along x, one for its speed upwards, one
    /// for z and one for the speed of its sprite. A full pool takes no piece and makes
    /// no draw for it.
    fn drawn_in(
        &mut self,
        package: &Package,
        number: u32,
        place: [i32; 3],
        draws: &mut impl Draws,
    ) {
        let Some(sprite) = package.find(Kind::Sprite, number) else {
            return;
        };
        for _ in 0..PIECES_OF_ROUTINE_24 {
            if !self.small_free() {
                continue;
            }
            let x = ((draws.draw() & 127) as i32 - 63) * 1024;
            let y = -((draws.draw() & 63) as i32) * 1024;
            let z = ((draws.draw() & 127) as i32 - 63) * 1024;
            let speed = 2048 + (draws.draw() & 4095) as i32 / 2;
            self.pieces.push(Piece::Pulled {
                sprite,
                counter: 0,
                speed,
                place,
                moves: [x, y, z],
            });
        }
    }

    /// Whether the pool of the small tasks has a free node.
    fn small_free(&self) -> bool {
        self.small.len() + self.pieces.len() + self.held.len() + self.held_sounds.len()
            < SMALL_TASKS
    }

    /// The maker of routine 6 (`0x800C2AE4(task, number)`): a small task that counts
    /// to the break at the party's place, and eight pieces of the model's first mesh
    /// object on the upper half of a ball of [`PIECES_FAR`] round it, an eighth of a
    /// turn apart, each with a draw for its size, one for its height and one for its
    /// third angle, heading for the place a quarter of the way a step.
    fn pieces(
        &mut self,
        package: &Package,
        number: u32,
        place: [i32; 3],
        field: &impl Field,
        draws: &mut impl Draws,
    ) {
        if self.small_free() {
            self.pieces.push(Piece::Gather { counter: 0, place });
        }
        for piece in 0..PIECES {
            if !self.small_free() {
                continue;
            }
            let scale = 2048 + (draws.draw() & 0x7FF) as i32;
            let round = piece * 512;
            let far = |angle: i32| PIECES_FAR * sine(angle) / 4096;
            let (x, z) = (far(round), far(round + 1024));
            let up = (draws.draw() & 0x3FF) as i32;
            let y = -(PIECES_FAR * sine(up)) / 4096;
            let level = sine(up + 1024);
            let offset = [x * level / 4096, y, z * level / 4096].map(|part| part << 10);
            let speed = offset.map(|part| -part / 4);
            let mut angles = heading(field, speed.map(|part| part / 1024));
            angles[2] = (draws.draw() & 0x1000) as i16;
            let Some(model) = package.find(Kind::Model, number) else {
                continue;
            };
            self.pieces.push(Piece::Closing {
                counter: 0,
                model,
                origin: place,
                offset,
                speed,
                angles,
                scale,
            });
        }
    }

    /// The break (`0x800C25A0`, the thirtieth step of its task): the sound, the
    /// sprite of the break played once at the place, and 24 shards, a 24th of a turn
    /// apart and thrown up and outwards, each one of the first eight mesh objects of
    /// the shards' model in turn, with a draw for its size, four for its speed and
    /// four for its angles and its spin.
    fn shards(&mut self, package: &Package, place: [i32; 3], draws: &mut impl Draws) {
        self.asked.push(Asks::Sound(SHARD_SOUND));
        self.sprite(package, thing::BREAK, place, [0; 3]);
        for shard in 0..SHARDS {
            if !self.small_free() {
                continue;
            }
            let scale = 2048 + (draws.draw() & 0x7FF) as i32;
            let round = shard * 4096 / SHARDS;
            let x = (40 + (draws.draw() & 31) as i32) * sine(round) / 4096;
            let z = (40 + (draws.draw() & 31) as i32) * sine(round + 1024) / 4096;
            let up = (draws.draw() & 0x1FF) as i32;
            let y = -(40 + (draws.draw() & 31) as i32) * sine(up + 512) / 4096;
            let level = sine(up + 1536);
            let speed = [x * level / 4096, y, z * level / 4096].map(|part| part << 10);
            let angles = [(); 3].map(|()| (draws.draw() & 0x1000) as i16);
            let spin = 256 + (draws.draw() & 0x1FF) as i16;
            let Some(model) = package.find(Kind::Model, thing::SHARD) else {
                continue;
            };
            self.pieces.push(Piece::Shard {
                model,
                object: (shard % 8) as usize,
                origin: place,
                offset: [0; 3],
                speed,
                angles,
                spin,
                scale,
            });
        }
    }

    /// One game frame of a small task of routine 6; whether it lives on.
    fn piece(
        &mut self,
        piece: &mut Piece,
        package: &Package,
        ground: i32,
        actor: [i32; 3],
        draws: &mut impl Draws,
    ) -> bool {
        let at = |origin: [i32; 3], offset: [i32; 3]| {
            std::array::from_fn(|axis| origin[axis] + offset[axis] / 1024)
        };
        match piece {
            Piece::Gather { counter, place } => {
                *counter += 1;
                if *counter < PIECES_BREAK {
                    return true;
                }
                self.shards(package, *place, draws);
                false
            }
            Piece::Closing {
                counter,
                model,
                origin,
                offset,
                speed,
                angles,
                scale,
            } => {
                if (PIECES_CLOSE..PIECES_BREAK).contains(counter) {
                    for (part, by) in offset.iter_mut().zip(*speed) {
                        *part += by;
                    }
                }
                angles[2] = angles[2].wrapping_add(PIECE_TURN);
                self.drawn.push(Drawn::Piece {
                    model: *model,
                    object: 0,
                    place: at(*origin, *offset),
                    angles: *angles,
                    scale: *scale,
                });
                *counter += 1;
                *counter <= PIECES_BREAK
            }
            Piece::Rising {
                sprite,
                counter,
                speed,
                origin,
                offset,
                moves,
                angles,
            } => {
                // Every speed loses a thirteenth, and the piece is pulled upwards.
                *moves = moves.map(|part| part * 12 / 13);
                moves[1] -= 1024;
                angles[1] = angles[1].wrapping_add(FOUNTAIN_TURN);
                for (part, by) in offset.iter_mut().zip(*moves) {
                    *part += by / 1024;
                }
                let frame = (*counter / SPRITE_SPEED) as usize;
                if frame >= package.sprites[*sprite].frames.len() {
                    return false;
                }
                self.drawn.push(Drawn::Thrown {
                    sprite: *sprite,
                    frame,
                    place: *origin,
                    angles: *angles,
                    offset: offset.map(|part| part as i16),
                });
                *counter += *speed;
                true
            }
            Piece::Pulled {
                sprite,
                counter,
                speed,
                place,
                moves,
            } => {
                // Every speed loses a 32nd and gains 32 for each unit of the way that
                // is left to the actor's bone.
                for axis in 0..3 {
                    moves[axis] = moves[axis] * 31 / 32 + ((actor[axis] - place[axis]) << 5);
                }
                for (part, by) in place.iter_mut().zip(*moves) {
                    *part += by / 1024;
                }
                let frame = (*counter / SPRITE_SPEED) as usize;
                if frame >= package.sprites[*sprite].frames.len() {
                    return false;
                }
                self.drawn.push(Drawn::Thrown {
                    sprite: *sprite,
                    frame,
                    place: *place,
                    angles: [0; 3],
                    offset: [0; 3],
                });
                *counter += *speed;
                true
            }
            Piece::Falling {
                sprite,
                size,
                place,
                fall,
                turn,
            } => {
                // The fall loses a sixteenth and gains an eighth of the size; a piece
                // that is below the ground or smaller than a half is drawn once more.
                *fall = *fall * 15 / 16 + *size / 8;
                place[1] += *fall;
                let at = place.map(|part| part / 1024);
                let lives = at[1] <= ground && *size >= SHED_SMALLEST;
                *size -= SHED_SHRINKS;
                *turn = turn.wrapping_add(SHED_TURN);
                self.drawn.push(Drawn::Shed {
                    sprite: *sprite,
                    place: at,
                    angles: [0, *turn, 0],
                    size: *size,
                });
                lives
            }
            Piece::Shard {
                model,
                object,
                origin,
                offset,
                speed,
                angles,
                spin,
                scale,
            } => {
                speed[1] += 4096;
                *speed = speed.map(|part| part * 31 / 32);
                for (part, by) in offset.iter_mut().zip(*speed) {
                    *part += by;
                }
                // On the ground the shard turns round with three quarters of its
                // speed, and ends when that is too little.
                let mut lives = true;
                if offset[1] > 0 {
                    speed[1] = -speed[1] * 3 / 4;
                    offset[1] = 0;
                    lives = speed[1] < -4095;
                }
                angles[2] = angles[2].wrapping_add(*spin);
                self.drawn.push(Drawn::Piece {
                    model: *model,
                    object: *object,
                    place: at(*origin, *offset),
                    angles: *angles,
                    scale: *scale,
                });
                lives
            }
        }
    }

    /// The count of frames of a sprite of the package, the first word of its record;
    /// 0 for a package without it.
    fn sprite_frames(package: &Package, number: u32) -> i32 {
        (package.find(Kind::Sprite, number))
            .map_or(0, |sprite| package.sprites[sprite].frames.len() as i32)
    }

    /// A row of the fight's sequences held for a count of game frames (`0x800C1288`,
    /// a small task): started now, stopped by the step that finds the count below 0.
    /// Nothing for a count that is not above 0.
    fn hold(&mut self, count: i32, row: u32) {
        if count > 0 {
            self.asked.push(Asks::Row(row));
            self.held.push((row, count));
        }
    }

    /// A sound of a bank held for a count of game frames (`0x800C1360(count, number)`,
    /// a small task): started now (`0x80031550`), stopped by the step that finds the
    /// count below 0 (`0x800C1310`). Nothing for a count that is not above 0 or a
    /// full pool.
    fn hold_sound(&mut self, count: i32, number: u32) {
        if count > 0 && self.small_free() {
            self.asked.push(Asks::SoundHeld(number));
            self.held_sounds.push((number, count));
        }
    }

    /// The turn from a party towards the actor (`0x8003B9E4(party, actor)`): the arc
    /// tangent of the way along x over the way along z.
    fn towards_actor(field: &impl Field, who: Who) -> i16 {
        let (from, to) = (field.stands(who), field.stands(Who::Actor));
        field.arc(to[0] - from[0], to[2] - from[2])
    }

    pub fn pass(
        &mut self,
        package: &Package,
        field: &impl Field,
        draws: &mut impl Draws,
        loops_end: bool,
    ) {
        self.drawn.clear();
        // The frame routine has sent the picture of the game frame before and cleared
        // the bit (`0x8001B13C`).
        self.picture = None;
        self.waits = (self.waits - 1).max(0);
        for index in 0..TASKS {
            let Some(node) = self.tasks[index] else {
                continue;
            };
            self.tasks[index] = self.step(node, index, package, field, draws, loops_end);
        }
        // The small tasks of routine 6; the shards a break makes are stepped in its
        // pass, as its sprite is.
        let ground = field.ground();
        let actor = field.bone(Who::Actor, DRAWN_TO_BONE);
        let mut index = 0;
        while index < self.pieces.len() {
            let mut piece = self.pieces[index];
            match self.piece(&mut piece, package, ground, actor, draws) {
                true => {
                    self.pieces[index] = piece;
                    index += 1;
                }
                false => {
                    self.pieces.remove(index);
                }
            }
        }
        let mut small = std::mem::take(&mut self.small);
        small.retain_mut(|task| {
            if let Some(speed) = &mut task.moves {
                *speed = speed.map(|part| part * 15 / 16);
                speed[1] += 512;
                for (place, part) in task.place.iter_mut().zip(*speed) {
                    *place += part / 1024;
                }
                if task.place[1] > ground {
                    task.place[1] = ground;
                    speed[1] = -speed[1];
                }
            }
            let frames = package.sprites[task.sprite].frames.len();
            let frame = (task.counter / SPRITE_SPEED) as usize;
            if frame >= frames {
                return false;
            }
            self.drawn.push(Drawn::Sprite {
                sprite: task.sprite,
                frame,
                place: task.place,
                angles: task.angles,
            });
            task.counter += task.speed;
            ((task.counter / SPRITE_SPEED) as usize) < frames
        });
        self.small = small;
        // The step of a held row (`0x800C123C`): the count goes down, and below 0 the
        // row is stopped and the task ended. A task made in this pass is stepped in it.
        let mut held = std::mem::take(&mut self.held);
        held.retain_mut(|(row, count)| {
            *count -= 1;
            if *count < 0 {
                self.asked.push(Asks::RowStop(*row));
            }
            *count >= 0
        });
        self.held = held;
        let mut held = std::mem::take(&mut self.held_sounds);
        held.retain_mut(|(number, count)| {
            *count -= 1;
            if *count < 0 {
                self.asked.push(Asks::SoundStop(*number));
            }
            *count >= 0
        });
        self.held_sounds = held;
    }

    /// One game frame of a node; what is left of it.
    fn step(
        &mut self,
        node: Node,
        index: usize,
        package: &Package,
        field: &impl Field,
        draws: &mut impl Draws,
        loops_end: bool,
    ) -> Option<Node> {
        // +68 of the node's memory: a step and the light in the hand keep the place of
        // their point there, a light and the model that swells their own; the shell of
        // a capture and the task of its rings write nothing there. What the other
        // routines leave there is not read.
        match node {
            Node::Step(_) | Node::Glow(_) | Node::Rings(_) | Node::Shell { .. } => {}
            Node::Light { place, .. } | Node::Swell { place, .. } => {
                self.kept[index] = Some(place);
            }
            _ => self.kept[index] = None,
        }
        match node {
            // The clip's player (`0x80017B10`): the counter goes up first. Below the
            // end, and while the loops of a capture have not ended, the picture of
            // the counter modulo the clip's count is unpacked unless one was in this
            // game frame already. Else the bit is cleared, so that no picture of this
            // game frame is sent, and the task ends.
            Node::Clip { counter, end } => {
                let counter = counter + 1;
                if counter >= end || loops_end {
                    self.picture = None;
                    return None;
                }
                let pictures = package.clips.first().map_or(0, Vec::len);
                if self.picture.is_none() && pictures > 0 {
                    self.picture = Some(counter as usize % pictures);
                }
                Some(Node::Clip { counter, end })
            }
            Node::Step(mut task) => {
                task.before = task.at;
                task.at = field.point(task.who, task.bone, task.point);
                self.kept[index] = Some(task.at);
                if task.counter >= i32::from(task.start)
                    && let Some(other) = self.case(&mut task, index, package, field, draws)
                {
                    // A case that puts another routine into the task leaves through
                    // the exit that does not count.
                    return Some(other);
                }
                let ended = task.counter >= i32::from(task.end);
                task.counter += 1;
                (!ended).then_some(Node::Step(task))
            }
            Node::Rings(mut task) => {
                if loops_end {
                    self.asked.push(Asks::WayEnd(6));
                }
                task.own += 1;
                if task.own % 10 == 0 {
                    let place = field.stands(task.who);
                    self.model(package, thing::SHELL, place, 0, index);
                }
                (!loops_end).then_some(Node::Rings(task))
            }
            Node::Glow(mut task) => {
                task.own += 1;
                task.before = task.at;
                task.at = field.point(task.who, task.bone, task.point);
                self.kept[index] = Some(task.at);
                if loops_end {
                    return None;
                }
                if let Some(glow) = package.find(Kind::Sprite, thing::GLOW) {
                    self.drawn.push(Drawn::Glow {
                        glow,
                        flare: package.find(Kind::Sprite, thing::FLARE),
                        place: task.at,
                        count: task.own,
                    });
                }
                Some(Node::Glow(task))
            }
            Node::Model {
                model,
                frame,
                place,
                turn,
            } => {
                let frames = package.models[model].frames();
                if frame >= frames {
                    return None;
                }
                self.drawn.push(Drawn::Model {
                    model,
                    frame,
                    place,
                    turn,
                });
                (frame + 1 < frames).then_some(Node::Model {
                    model,
                    frame: frame + 1,
                    place,
                    turn,
                })
            }
            Node::Flying {
                sprite,
                model,
                frame,
                steps,
                place,
                turn,
                mut points,
            } => {
                let quads = package.sprites[sprite].quads(frame);
                if frame >= package.sprites[sprite].frames.len() {
                    return None;
                }
                let angles = [0, turn, 0];
                self.drawn.push(Drawn::Sprite {
                    sprite,
                    frame,
                    place,
                    angles,
                });
                // The first ring is where the frame's last quad stands; the first game
                // frame puts all four there.
                if let Some(quad) = quads.last() {
                    points[0] = quad.stands(place, angles);
                }
                if steps == 0 {
                    points = [points[0]; RINGS];
                }
                if let Some(model) = model {
                    self.drawn.push(Drawn::Ribbon {
                        model,
                        points,
                        angles,
                    });
                }
                // Every other game frame the rings move on by one.
                if steps & 1 == 0 {
                    points.copy_within(0..RINGS - 1, 1);
                }
                Some(Node::Flying {
                    sprite,
                    model,
                    frame: frame + 1,
                    steps: steps + 1,
                    place,
                    turn,
                    points,
                })
            }
            Node::Shedding {
                path,
                sprites,
                mut count,
                counter,
                place,
            } => {
                let frame = (counter / SPRITE_SPEED) as usize;
                if frame >= package.sprites[path].frames.len() {
                    return None;
                }
                if frame.is_multiple_of(SHED_EVERY) {
                    // Where the frame's first quad stands, without a turn.
                    let quads = package.sprites[path].quads(frame);
                    let from = quads.first().map_or([0; 3], |quad| quad.place);
                    let at: [i32; 3] =
                        std::array::from_fn(|axis| (place[axis] + i32::from(from[axis])) << 10);
                    count += 1;
                    let sprite = sprites[usize::from(count & 1 == 0)];
                    for third in 0..SHED {
                        if !self.small_free() {
                            continue;
                        }
                        let size = SHED_SMALLEST + (draws.draw() & 0xFFF) as i32;
                        let Some(sprite) = sprite else { continue };
                        self.pieces.push(Piece::Falling {
                            sprite,
                            size,
                            place: at,
                            fall: 0,
                            turn: third * SHED_APART,
                        });
                    }
                }
                Some(Node::Shedding {
                    path,
                    sprites,
                    count,
                    counter: counter + SPRITE_SPEED,
                    place,
                })
            }
            Node::Wobble {
                model,
                counter,
                size,
                growth,
                turn,
                place,
            } => {
                let growth = growth * 7 / 8;
                let size = size + growth;
                let turn = turn.wrapping_add(WOBBLE_TURN);
                // The share of the first shape swings round a half with the sine of an
                // eighth of a turn a game frame.
                let share = sine((counter & 7) * 512) / 2 + SHARE_ONE / 2;
                self.drawn.push(Drawn::Mixed {
                    model,
                    share,
                    place,
                    angles: [0, turn, 0],
                    scale: size / 4096,
                    fade: (counter + 1 - WOBBLE_FADES).max(0) * SHARE_STEP,
                });
                (counter < WOBBLE_LAST).then_some(Node::Wobble {
                    model,
                    counter: counter + 1,
                    size,
                    growth,
                    turn,
                    place,
                })
            }
            Node::Swell {
                model,
                counter,
                end,
                size,
                growth,
                place,
            } => {
                if counter >= end {
                    self.light = None;
                    return None;
                }
                // The growth loses an eighth a game frame, so the size comes to seven
                // times the first growth and no further.
                let growth = growth * 7 / 8;
                let size = size + growth;
                self.drawn.push(Drawn::Piece {
                    model,
                    object: 0,
                    place,
                    angles: [0; 3],
                    scale: size / 4096,
                });
                // The light dims from full to nothing over the routine's life.
                self.shine(place, SWELL_LIGHT, (end - counter) * 4096 / end);
                Some(Node::Swell {
                    model,
                    counter: counter + 1,
                    end,
                    size,
                    growth,
                    place,
                })
            }
            Node::Cover {
                model,
                who,
                bone,
                scale,
                mut left,
                mut stays,
            } => {
                match stays {
                    // The game frame that finds the share whole hands the task to the
                    // second routine and still draws.
                    None => {
                        left -= COVER_STEP;
                        if left <= 0 {
                            (left, stays) = (0, Some(SHARE_ONE));
                        }
                    }
                    Some(count) => {
                        if count - COVER_STEP <= 0 {
                            return None;
                        }
                        stays = Some(count - COVER_STEP);
                    }
                }
                self.drawn.push(Drawn::Shape {
                    model,
                    share: SHARE_ONE - left,
                    place: field.bone(who, bone),
                    scale,
                    whole: true,
                });
                Some(Node::Cover {
                    model,
                    who,
                    bone,
                    scale,
                    left,
                    stays,
                })
            }
            Node::Ending { left } => {
                if left - 1 < 0 {
                    self.asked.push(Asks::Sound(SOUND_AFTER_ROUTINE_39));
                    return None;
                }
                Some(Node::Ending { left: left - 1 })
            }
            Node::Light {
                record,
                place,
                counter,
            } => {
                if counter >= LIGHT_OF_ROUTINE_24 {
                    self.light = None;
                    return None;
                }
                if let Some(&colour) = LIGHT_COLOURS.get(record) {
                    self.shine(place, colour, (LIGHT_OF_ROUTINE_24 - counter) * 128);
                }
                Some(Node::Light {
                    record,
                    place,
                    counter: counter + 1,
                })
            }
            Node::Timed {
                routine,
                who,
                place,
                counter,
            } => {
                let turn = Self::towards_actor(field, who);
                let party = field.stands(who);
                match (routine, counter) {
                    // Five sprites and five models round the place, a fifth of a
                    // turn apart, the first towards the actor.
                    (40, ROUTINE_40_STARTS) => {
                        self.asked.push(Asks::Way(6));
                        for number in SOUNDS_OF_ROUTINE_40 {
                            self.asked.push(Asks::Sound(number));
                        }
                        for fifth in 0..ROUND_ROUTINE_40 {
                            let angle = turn.wrapping_add((fifth * 4096 / ROUND_ROUTINE_40) as i16);
                            let sprite = thing::ROUND_SPRITE_OF_ROUTINE_40;
                            self.sprite(package, sprite, place, [0, angle, 0]);
                            let model = thing::ROUND_MODEL_OF_ROUTINE_40;
                            self.model(package, model, place, angle, index);
                        }
                    }
                    (40, ROUTINE_40_ENDS) => {
                        self.asked.push(Asks::WayEnd(6));
                        self.asked.push(Asks::Sound(SOUND_AFTER_ROUTINE_40));
                        let model = thing::LAST_MODEL_OF_ROUTINE_40;
                        let frames = self.model(package, model, place, turn, index);
                        self.clip(package, frames, index);
                        return None;
                    }
                    // Two held sounds, and four sprites at the place and four models
                    // at the party's, a quarter of a turn apart; a model stands a
                    // quarter further round than the sprite made with it.
                    (47, 0) => {
                        for (count, number) in HELD_BY_ROUTINE_47 {
                            self.hold_sound(count, number);
                        }
                        for quarter in 0..ROUND_ROUTINE_47 {
                            let angle = turn.wrapping_add(quarter * 1024);
                            let sprite = thing::ROUND_SPRITE_OF_ROUTINE_47;
                            self.sprite(package, sprite, place, [0, angle, 0]);
                            let model = thing::ROUND_MODEL_OF_ROUTINE_47;
                            self.model(package, model, party, angle.wrapping_add(1024), index);
                        }
                    }
                    (47, ROUTINE_47_SECOND) => {
                        let sprite = thing::SECOND_SPRITE_OF_ROUTINE_47;
                        self.sprite(package, sprite, place, [0, turn, 0]);
                    }
                    (47, ROUTINE_47_ENDS) => {
                        let model = thing::LAST_MODEL_OF_ROUTINE_47;
                        self.model(package, model, party, turn, index);
                        return None;
                    }
                    _ => {}
                }
                Some(Node::Timed {
                    routine,
                    who,
                    place,
                    counter: counter + 1,
                })
            }
            Node::Shell {
                who,
                bone,
                scale,
                mut share,
                mut counter,
            } => {
                let model = package.find(Kind::Model, thing::SHELL)?;
                let place = field.bone(who, bone);
                // The three words are read through the task's pointer in every game
                // frame, so the shell has the size the target has now.
                let now = field.scale(who, scale);
                let shape = |share| Drawn::Shape {
                    model,
                    share,
                    place,
                    scale: now,
                    whole: true,
                };
                // The light of the shell is set where +68 of the task's memory says,
                // which the shell never writes: at what the task before it in this
                // node left there. Left out when the build does not know that.
                let lit = self.kept[index];
                if counter < CAPTURE_FRAMES && !loops_end {
                    share = (share + SHARE_STEP).min(SHARE_ONE);
                    self.drawn.push(shape(share));
                    if let Some(lit) = lit {
                        self.shine(lit, SHELL_LIGHT, share);
                    }
                    counter += 1;
                } else {
                    self.drawn.push(shape(share));
                    if share == SHARE_ONE {
                        self.follow(field, place, index);
                    }
                    if let Some(lit) = lit {
                        self.shine(lit, SHELL_LIGHT, share);
                    }
                    if share <= 0 {
                        self.light = None;
                        return None;
                    }
                    share -= SHARE_STEP;
                }
                Some(Node::Shell {
                    who,
                    bone,
                    scale,
                    share,
                    counter,
                })
            }
            Node::Spark {
                mut place,
                mut speed,
            } => {
                self.sprite(package, thing::GLOW, place, [0; 3]);
                let goal = field.hand();
                if squared_distance(place, goal) <= 22500 {
                    self.burst(package, thing::GLOW, place, draws);
                    return None;
                }
                for axis in 0..3 {
                    speed[axis] += (goal[axis] - place[axis]) * 1024 / 70;
                    speed[axis] = speed[axis] * 31 / 32;
                    place[axis] += speed[axis] / 1024;
                }
                Some(Node::Spark { place, speed })
            }
            Node::Broken { place, mut share } => {
                share -= SHARE_STEP;
                if share <= 0 {
                    return None;
                }
                if let Some(model) = package.find(Kind::Model, thing::BROKEN) {
                    self.drawn.push(Drawn::Shape {
                        model,
                        share,
                        place,
                        scale: None,
                        whole: false,
                    });
                }
                Some(Node::Broken { place, share })
            }
        }
    }

    /// The follow-up of a shell that had grown whole (`0x800C3D20`).
    fn follow(&mut self, field: &impl Field, place: [i32; 3], busy: usize) {
        match self.caught {
            true => {
                self.asked.push(Asks::Sound(71));
                self.take(
                    Node::Spark {
                        place,
                        speed: [0, -200, 0],
                    },
                    Some(busy),
                );
            }
            false => {
                self.asked.push(Asks::Sound(63));
                self.take(
                    Node::Broken {
                        place: field.hand(),
                        share: SHARE_ONE,
                    },
                    Some(busy),
                );
            }
        }
    }

    /// The case of a task's routine. Answers the node the task goes on as when the case
    /// has put another routine into it.
    fn case(
        &mut self,
        task: &mut Task,
        index: usize,
        package: &Package,
        field: &impl Field,
        draws: &mut impl Draws,
    ) -> Option<Node> {
        match task.routine {
            // The hit (`0x80015CF0`): a sprite played once and a burst at the point,
            // unless the action missed its party.
            1 if task.byte & MISSED == 0 => {
                self.sprite(package, thing::HIT, task.at, [0; 3]);
                self.burst(package, thing::HIT_BURST, task.at, draws);
            }
            // The trail: a sprite for every 128 units the point has moved.
            0 if task.counter != 0 => {
                task.own += root(squared_distance(task.at, task.before));
                if task.own > 128 {
                    let pieces = task.own / 128;
                    for piece in 0..pieces {
                        let place = std::array::from_fn(|axis| {
                            let way = task.at[axis].wrapping_sub(task.before[axis]);
                            task.before[axis].wrapping_add(way.wrapping_mul(piece) / pieces)
                        });
                        task.own -= 128;
                        self.sprite(package, thing::TRAIL, place, [0; 3]);
                    }
                }
            }
            // The main piece of a spell (`0x8001621C`): row 14 of the fight's
            // sequences, the pieces round the party's place and the clip.
            6 => {
                self.asked.push(Asks::Row(14));
                let place = field.stands(task.who);
                self.pieces(package, thing::PIECE, place, field, draws);
                self.clip(package, PIECES_CLIP, index);
            }
            // The main piece of a spell (`0x80016330`): row 16 of the fight's
            // sequences, two sprites played once at the point, and the clip for as
            // many game frames as the first sprite has frames.
            8 => {
                self.asked.push(Asks::Row(16));
                for number in thing::OF_ROUTINE_8 {
                    self.sprite(package, number, task.at, [0; 3]);
                }
                let frames = Self::sprite_frames(package, thing::OF_ROUTINE_8[0]);
                self.clip(package, frames, index);
            }
            // The main piece of a spell (`0x800165E4`): row 18, two sprites played
            // once at the point, the clip for the first sprite's frames and a model
            // played once at the party's place.
            29 => {
                self.asked.push(Asks::Row(18));
                for number in thing::OF_ROUTINE_29 {
                    self.sprite(package, number, task.at, [0; 3]);
                }
                let frames = Self::sprite_frames(package, thing::OF_ROUTINE_29[0]);
                self.clip(package, frames, index);
                let place = field.stands(task.who);
                self.model(package, thing::MODEL_OF_ROUTINE_29, place, 0, index);
            }
            // The main piece of a spell (`0x80016758`): row 23, a burst at the point
            // and two models played once at the party's place.
            33 => {
                self.asked.push(Asks::Row(23));
                self.burst(package, thing::BURST_OF_ROUTINE_33, task.at, draws);
                let place = field.stands(task.who);
                for number in thing::OF_ROUTINE_33 {
                    self.model(package, number, place, 0, index);
                }
            }
            // The main piece of a spell (`0x80016248`): row 22, and five times, a
            // fifth of a turn apart, a sprite played once at the point and a model
            // played once at the party's place; the clip for the sprite's frames.
            9 => {
                self.asked.push(Asks::Row(22));
                let place = field.stands(task.who);
                for fifth in 0..5 {
                    let turn = (fifth * 4096 / 5) as i16;
                    self.sprite(package, thing::ROUND_SPRITE, task.at, [0, turn, 0]);
                    self.model(package, thing::ROUND_MODEL, place, turn, index);
                }
                let frames = Self::sprite_frames(package, thing::ROUND_SPRITE);
                self.clip(package, frames, index);
            }
            // The main piece of two spells (`0x800166C4`, `0x800167D0`): a model played
            // once at the party's place and the clip for as many game frames as the
            // model has frames; routine 7 holds row 13 of the fight's sequences for as
            // long, routine 31 starts row 15.
            7 => {
                let place = field.stands(task.who);
                let frames = self.model(package, thing::OF_ROUTINE_7, place, 0, index);
                self.clip(package, frames, index);
                self.hold(frames, 13);
            }
            31 => {
                self.asked.push(Asks::Row(15));
                let place = field.stands(task.who);
                let frames = self.model(package, thing::OF_ROUTINE_31, place, 0, index);
                self.clip(package, frames, index);
            }
            // The main piece of a spell (`0x80016178`): a model of two shapes that
            // swells at the party's place (`0x800174F8`), the clip for 100 game frames,
            // row 19 of the fight's sequences, and a fountain of twelve sprites at the
            // point; thirty more on a party that took what was done to it.
            30 => {
                if let Some(model) = package.find(Kind::Model, thing::WOBBLE) {
                    self.take(
                        Node::Wobble {
                            model,
                            counter: 0,
                            size: 0,
                            growth: SWELL_GROWTH,
                            turn: 0,
                            place: field.stands(task.who),
                        },
                        Some(index),
                    );
                }
                self.clip(package, FOUNTAIN_CLIP, index);
                self.asked.push(Asks::Row(19));
                self.fountain(package, thing::FOUNTAIN, task.at, 0, FOUNTAINS[0], draws);
                if task.byte & REFUSED == 0 {
                    self.fountain(package, thing::FOUNTAIN, task.at, 0, FOUNTAINS[1], draws);
                }
            }
            // The main piece of a spell (`0x80016408`): row 20 of the fight's
            // sequences, six lights that fly in round the party's place, a sixth of a
            // turn apart (`0x800C2FF8`), and a model played once there.
            20 => {
                self.asked.push(Asks::Row(20));
                let place = field.stands(task.who);
                if let Some(sprite) = package.find(Kind::Sprite, thing::FLYING) {
                    let model = package.find(Kind::Model, thing::RIBBON);
                    for light in 0..FLYING {
                        self.take(
                            Node::Flying {
                                sprite,
                                model,
                                frame: 0,
                                steps: 0,
                                place,
                                turn: (light * 4096 / FLYING) as i16,
                                points: [place; RINGS],
                            },
                            Some(index),
                        );
                    }
                }
                self.model(package, thing::OF_ROUTINE_20, place, 0, index);
            }
            // A fountain of thirty sprites at the point (`0x800164A0`).
            13 => {
                let number = thing::FOUNTAIN_OF_ROUTINE_13;
                self.fountain(package, number, task.at, 0, FOUNTAINS[1], draws);
            }
            // The main piece of four spells for the own side (`0x800165CC`): row 21 of
            // the fight's sequences, a model played once at the party's place and the
            // clip for as many game frames as the model has frames.
            28 | 34 | 35 | 36 => {
                self.asked.push(Asks::Row(21));
                let place = field.stands(task.who);
                let frames = self.model(package, thing::OF_ROUTINE_28, place, 0, index);
                self.clip(package, frames, index);
            }
            // The main piece of a spell (`0x80016118`): row 19, a model played once at
            // the point with the clip for its frames, and a fountain of thirty sprites
            // there on a party that took what was done to it.
            49 => {
                self.asked.push(Asks::Row(19));
                let frames = self.model(package, thing::OF_ROUTINE_49, task.at, 0, index);
                self.clip(package, frames, index);
                if task.byte & REFUSED == 0 {
                    let number = thing::FOUNTAIN_OF_ROUTINE_49;
                    self.fountain(package, number, task.at, 0, FOUNTAINS[1], draws);
                }
            }
            // The main piece of three effects (`0x80016990`, `0x8001691C`,
            // `0x80016A34`): row 20, a sprite played once at the point, and the task
            // goes on as a place that takes the sprite's path and sheds pieces
            // (`0x800C1620`).
            41..=43 => {
                self.asked.push(Asks::Row(20));
                let [path, odd, even] = match task.routine {
                    41 => thing::OF_ROUTINE_41,
                    42 => thing::OF_ROUTINE_42,
                    _ => thing::OF_ROUTINE_43,
                };
                self.sprite(package, path, task.at, [0; 3]);
                if let Some(path) = package.find(Kind::Sprite, path) {
                    return Some(Node::Shedding {
                        path,
                        sprites: [odd, even].map(|number| package.find(Kind::Sprite, number)),
                        count: task.counter,
                        counter: 0,
                        place: task.at,
                    });
                }
            }
            // The main piece of a spell (`0x80016540`): the task goes on as a model of
            // two shapes that grows round the point's bone (`0x80017E30`), with a
            // sound and the clip for 64 game frames.
            27 => {
                if let Some(model) = package.find(Kind::Model, thing::COVER) {
                    self.asked.push(Asks::Sound(COVER_SOUND));
                    self.clip(package, COVER_CLIP, index);
                    return Some(Node::Cover {
                        model,
                        who: task.who,
                        bone: task.bone,
                        scale: task.scale,
                        left: SHARE_ONE,
                        stays: None,
                    });
                }
            }
            // The main piece of two spells (`0x80015F38`, `0x80015FF4`): row 19, the
            // clip for as many game frames as a sprite has frames, that sprite played
            // once at the point, for routine 50 a fountain of twelve sprites there,
            // and a fountain of thirty on a party that took what was done to it.
            48 | 50 => {
                self.asked.push(Asks::Row(19));
                let (once, first, second) = match task.routine {
                    48 => (thing::OF_ROUTINE_48[0], None, thing::OF_ROUTINE_48[1]),
                    _ => {
                        let [once, first, second] = thing::OF_ROUTINE_50;
                        (once, Some(first), second)
                    }
                };
                self.clip(package, Self::sprite_frames(package, once), index);
                self.sprite(package, once, task.at, [0; 3]);
                if let Some(first) = first {
                    self.fountain(package, first, task.at, 0, FOUNTAINS[0], draws);
                }
                if task.byte & REFUSED == 0 {
                    self.fountain(package, second, task.at, 0, FOUNTAINS[1], draws);
                }
            }
            // The main piece of a spell (`0x80016804`): three sounds, a sprite played
            // once at the point and two models played once at the party's place.
            38 => {
                for number in SOUNDS_OF_ROUTINE_38 {
                    self.asked.push(Asks::Sound(number));
                }
                self.sprite(package, thing::SPRITE_OF_ROUTINE_38, task.at, [0; 3]);
                let place = field.stands(task.who);
                for number in thing::MODELS_OF_ROUTINE_38 {
                    self.model(package, number, place, 0, index);
                }
            }
            // The main piece of a spell (`0x80016888`): a held sound, a sprite played
            // once at the point, a model played once at the party's place and turned
            // towards the actor; the task counts three game frames and ends with a
            // sound (`0x80016BF4`).
            39 => {
                let (count, number) = HELD_BY_ROUTINE_39;
                self.hold_sound(count, number);
                self.sprite(package, thing::SPRITE_OF_ROUTINE_39, task.at, [0; 3]);
                let turn = Self::towards_actor(field, task.who);
                let place = field.stands(task.who);
                self.model(package, thing::MODEL_OF_ROUTINE_39, place, turn, index);
                return Some(Node::Ending {
                    left: ROUTINE_39_COUNTS,
                });
            }
            // The main piece of two spells (`0x80016B34`, `0x80016B40`): the task
            // goes on as a count of game frames (`0x80016E20`, `0x80016FC8`); routine
            // 47 asks for the clip for 100 game frames first.
            40 | 47 => {
                if task.routine == 47 {
                    self.clip(package, ROUTINE_47_CLIP, index);
                }
                return Some(Node::Timed {
                    routine: task.routine,
                    who: task.who,
                    place: task.at,
                    counter: 0,
                });
            }
            // A summon and a leave (`0x80015750`), every game frame of the window: a
            // model played once at the party's place at the start, one every twelfth
            // game frame of the action and two every ninth.
            19 | 44 => {
                let place = field.stands(task.who);
                let [first, twelfth, third, sixth] = thing::OF_SUMMON;
                let (start, end) = (i32::from(task.start), i32::from(task.end));
                if task.counter == start {
                    self.model(package, first, place, 0, index);
                    if task.counter < end {
                        self.asked.push(Asks::Way(6));
                    }
                }
                if task.counter % SUMMON_EVERY == 0 {
                    self.model(package, twelfth, place, 0, index);
                }
                let (every, [at_third, at_sixth]) = SUMMON_OTHERS;
                if task.counter % every == at_third {
                    self.model(package, third, place, 0, index);
                }
                if task.counter % every == at_sixth {
                    self.model(package, sixth, place, 0, index);
                }
                if task.counter >= end {
                    self.asked.push(Asks::WayEnd(6));
                }
            }
            // A sprite played once at the point, every game frame of the window
            // (`0x80015908`).
            21 => self.sprite(package, thing::OF_ROUTINE_21, task.at, [0; 3]),
            // The same with another sprite (`0x80015934`); the task that runs in the
            // first game frame of the action holds a sound for the length of its
            // window, unless a task of this effect holds it already.
            // Routine 23 (`0x800159A0`) holds the sound as 52 does and plays the sprite
            // of the half word of a move, two frames a game frame.
            23 | 52 => {
                if task.counter == 0 && !self.window_sound {
                    self.window_sound = true;
                    let length = i32::from(task.end) - i32::from(task.start);
                    self.hold_sound(length, WINDOW_SOUND);
                }
                match task.routine {
                    52 => self.sprite(package, thing::OF_ROUTINE_52, task.at, [0; 3]),
                    _ => {
                        if let Some(number) = sprite_of_half(task.half) {
                            self.sprite_at(package, number, task.at, [0; 3], FAST_SPRITE);
                        }
                    }
                }
            }
            // The hit of a body attack and of some of the player's attacks
            // (`0x80015AE8`), on a party that was not missed: by the half word of a
            // move a model played once at the point, row 19 of the fight's sequences,
            // thirty pieces of a sprite that rise as a fountain or are drawn to the
            // actor, and a light. The task's half word is -1 from then on.
            24 if task.byte & REFUSED == 0 => {
                let hit = hit_of_half(task.half);
                task.half = 0xFF;
                if let Some(hit) = hit {
                    if hit.model {
                        self.model(package, thing::OF_ROUTINE_24, task.at, 0, index);
                    }
                    if let Some(number) = hit.sprite {
                        self.asked.push(Asks::Row(ROW_OF_ROUTINE_24));
                        match hit.drawn_in {
                            true => self.drawn_in(package, number, task.at, draws),
                            false => {
                                let count = PIECES_OF_ROUTINE_24;
                                self.fountain(package, number, task.at, 0, count, draws);
                            }
                        }
                        self.take(
                            Node::Light {
                                record: hit.light,
                                place: task.at,
                                counter: 0,
                            },
                            Some(index),
                        );
                    }
                }
            }
            // Four routines with one case (`0x8001652C`) and a fifth beside it
            // (`0x800164FC`): a model played once at the party's place and the clip
            // for as many game frames as the model has frames; the four play a sound
            // first.
            11 | 12 | 15 | 16 | 17 => {
                let number = match task.routine {
                    12 => thing::OF_ROUTINE_12,
                    _ => {
                        self.asked.push(Asks::Sound(SOUND_OF_ROUTINE_17));
                        thing::OF_ROUTINE_17
                    }
                };
                let place = field.stands(task.who);
                let frames = self.model(package, number, place, 0, index);
                self.clip(package, frames, index);
            }
            // A shield that went (`0x80016B60`): its sound and a model played once at
            // the point, on a party that lost a layer of its shield.
            51 if task.byte & SHIELD_WENT != 0 => {
                self.asked.push(Asks::Sound(SHIELD_SOUND));
                task.own = 16;
                self.model(package, thing::SHIELD, task.at, 0, index);
            }
            // The main piece of a spell (`0x800166F0`): the clip once through, a model
            // that swells at the party's place for as many game frames as the clip has
            // pictures (`0x80017290`), a sprite played once at the point, and row 13
            // of the fight's sequences held for 53 game frames.
            32 => {
                let pictures = package.clips.first().map_or(0, |clip| clip.len() as i32);
                self.clip(package, pictures, index);
                if let Some(model) = package.find(Kind::Model, thing::SWELL) {
                    self.take(
                        Node::Swell {
                            model,
                            counter: 0,
                            end: pictures,
                            size: 0,
                            growth: SWELL_GROWTH,
                            place: field.stands(task.who),
                        },
                        Some(index),
                    );
                }
                self.sprite(package, thing::SPRITE_OF_ROUTINE_32, task.at, [0; 3]);
                self.hold(SWELL_ROW_FRAMES, 13);
            }
            // Three sprites and three models round the point.
            3 => {
                for third in 0..3 {
                    let turn = (third * 4096 / 3) as i16;
                    self.sprite(package, thing::AROUND_SPRITE, task.at, [0, turn, 0]);
                    self.model(package, thing::AROUND_MODEL, task.at, turn, index);
                }
            }
            // The shell of a capture: a task of its own.
            4 => {
                self.asked.push(Asks::Row(9));
                self.take(
                    Node::Shell {
                        who: task.who,
                        bone: task.bone,
                        scale: task.scale,
                        share: 0,
                        counter: 0,
                    },
                    Some(index),
                );
            }
            // The cast: a model at the fighter's place when the window starts with the
            // action.
            18 | 37 => {
                if task.counter == 0 {
                    if task.routine == 37 || task.end != 0 {
                        self.asked.push(Asks::Way(6));
                    }
                    let number = match task.routine {
                        18 => thing::CAST,
                        _ => thing::CAST_OTHER,
                    };
                    self.model(package, number, field.stands(task.who), 0, index);
                } else if task.counter == i32::from(task.end) {
                    self.asked.push(Asks::WayEnd(6));
                }
            }
            25 => {
                self.asked.push(Asks::Sound(80));
                self.clip(package, CAPTURE_FRAMES, index);
                return Some(Node::Glow(*task));
            }
            26 => {
                self.asked.push(Asks::Way(6));
                return Some(Node::Rings(*task));
            }
            _ => {}
        }
        None
    }

    /// What the last pass shows.
    pub fn drawn(&self) -> &[Drawn] {
        &self.drawn
    }

    pub fn picture(&self) -> Option<usize> {
        self.picture
    }

    /// What the passes since the last call asked of the sound and the clip.
    pub fn take_asked(&mut self) -> Vec<Asks> {
        std::mem::take(&mut self.asked)
    }

    /// The loader would let the package go (its state 3): no task of either pool lives
    /// and its count is 0.
    pub fn is_over(&self) -> bool {
        self.waits == 0
            && self.small.is_empty()
            && self.pieces.is_empty()
            && self.held.is_empty()
            && self.held_sounds.is_empty()
            && self.tasks.iter().all(Option::is_none)
    }

    /// The routines tasks were started with whose cases are not built: their tasks only
    /// count to their end.
    pub fn unread(&self) -> &[u32] {
        &self.unread
    }

    /// How many effect tasks and small tasks live.
    pub fn living(&self) -> (usize, usize) {
        (
            self.tasks.iter().flatten().count(),
            self.small.len() + self.pieces.len(),
        )
    }
}

pub fn mix(first: [i16; 3], second: [i16; 3], share: i32) -> [i16; 3] {
    std::array::from_fn(|axis| {
        let (to, from) = (i32::from(first[axis]), i32::from(second[axis]));
        (from + (((to - from) * share) >> 12)) as i16
    })
}

pub fn shell_scale(scale: Option<[i32; 3]>) -> [i32; 3] {
    match scale {
        Some(scale) => scale.map(|part| part * SHELL_HALVES / 2),
        None => [SHARE_ONE; 3],
    }
}

/// A ring of the lens flare of the light in the hand: its place and its size, 4096
/// being the quad's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    pub place: [i32; 3],
    pub size: i32,
}

/// The table of the library's routine `0x8005B3CC` in the executable, and its count
/// of half words.
pub const UNIT_TABLE: u32 = 0x8008_8C28;
pub const UNIT_STEPS: usize = 192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit(Vec<i16>);

impl Unit {
    /// The table of an executable loaded at `base`.
    pub fn from_image(image: &[u8], base: u32) -> Option<Self> {
        let start = usize::try_from(UNIT_TABLE.checked_sub(base)?).ok()?;
        let bytes = image.get(start..start + UNIT_STEPS * 2)?;
        let (steps, _) = bytes.as_chunks::<2>();
        Some(Self(
            steps.iter().map(|pair| i16::from_le_bytes(*pair)).collect(),
        ))
    }

    /// A table of the given steps.
    pub fn of(steps: Vec<i16>) -> Self {
        Self(steps)
    }

    pub fn steps(&self) -> &[i16] {
        &self.0
    }

    /// The way at the length 4096. The console takes each part as a half word, sums
    /// the three squares, brings the sum to a number from 64 to 255 by an even count
    /// of bits, reads the table's step of that number, multiplies each part by it and
    /// shifts the product down by half of what the sum was shifted by. Nothing for a
    /// way of no length or a sum the console's addition could not hold.
    pub fn way(&self, way: [i32; 3]) -> Option<[i32; 3]> {
        let parts = way.map(|part| i32::from(part as i16));
        let sum: i64 = parts
            .iter()
            .map(|&part| i64::from(part) * i64::from(part))
            .sum();
        let sum = i32::try_from(sum).ok().filter(|&sum| sum > 0)?;
        let zeros = sum.leading_zeros() as i32 & !1;
        let down = (31 - zeros) >> 1;
        let number = match zeros >= 24 {
            true => sum << (zeros - 24),
            false => sum >> (24 - zeros),
        };
        let step = i32::from(*self.0.get(usize::try_from(number - 64).ok()?)?);
        Some(parts.map(|part| (step * part) >> down))
    }
}

pub fn flare(
    light: [i32; 3],
    eye: [i32; 3],
    looks: [i32; 3],
    distance: i32,
    unit: &Unit,
) -> Vec<Ring> {
    let Some(way) = unit.way(std::array::from_fn(|k| looks[k].wrapping_sub(eye[k]))) else {
        return Vec::new();
    };
    let way = way.map(i64::from);
    let along: i64 = (0..3)
        .map(|k| (i64::from(light[k]) - i64::from(eye[k])) * way[k])
        .sum::<i64>()
        / i64::from(SHARE_ONE);
    let centre: [i64; 3] =
        std::array::from_fn(|k| i64::from(eye[k]) + way[k] * along / (2 * i64::from(SHARE_ONE)));
    let to_light: [i64; 3] = std::array::from_fn(|k| i64::from(light[k]) - centre[k]);
    let at = |share: i64, of: i64| -> [i32; 3] {
        std::array::from_fn(|k| (centre[k] + to_light[k] * share / of) as i32)
    };
    let h = i64::from(distance);
    let mut rings = Vec::new();
    if h < along {
        rings.push(Ring {
            place: at(0, 1),
            size: SHARE_ONE / 4,
        });
    }
    if 4 * h / 3 < along {
        rings.push(Ring {
            place: at(1, 2),
            size: SHARE_ONE,
        });
    }
    if 2 * h < along {
        rings.push(Ring {
            place: at(-1, 2),
            size: SHARE_ONE / 2,
        });
    }
    if 4 * h / 3 < along {
        rings.push(Ring {
            place: at(-1, 4),
            size: SHARE_ONE * 2,
        });
    }
    rings
}

/// The two sizes of the light in the hand in the `count`-th game frame of its routine,
/// 4096 being its own: 1 plus a quarter of the sine of [`GLOW_SWELL`] times the count.
pub fn glow_size(count: i32) -> i32 {
    SHARE_ONE + crate::skeleton::sine(GLOW_SWELL * count) / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::MeshBlock;
    use crate::package::{DATA_SIZE, PLACEMENT_SIZE, PackageModel, Record, START_SIZE, Sprite};

    /// A package whose first section has these records (routine, on whom, point kinds),
    /// sprites with these numbers and counts of frames, and models likewise.
    fn package(
        starts: &[(usize, i32, i32, &[i16])],
        sprites: &[(u32, usize)],
        models: &[(u32, usize)],
    ) -> Package {
        let mut data = vec![0xFFu8; DATA_SIZE];
        for &(record, routine, whom, kinds) in starts {
            let at = START_SIZE * record;
            data[at..at + 4].copy_from_slice(&routine.to_le_bytes());
            data[at + 4..at + 8].copy_from_slice(&whom.to_le_bytes());
            for (index, kind) in kinds.iter().enumerate() {
                data[at + 8 + 2 * index..at + 10 + 2 * index].copy_from_slice(&kind.to_le_bytes());
            }
        }
        let mut records = Vec::new();
        for (index, &(number, _)) in sprites.iter().enumerate() {
            records.push(Record {
                kind: Kind::Sprite,
                number,
                index,
            });
        }
        for (index, &(number, _)) in models.iter().enumerate() {
            records.push(Record {
                kind: Kind::Model,
                number,
                index,
            });
        }
        Package {
            data,
            records,
            sprites: (sprites.iter())
                .map(|&(_, frames)| Sprite {
                    frames: vec![Vec::new(); frames],
                    description: [0; 40],
                })
                .collect(),
            models: (models.iter())
                .map(|&(_, frames)| PackageModel {
                    mesh: MeshBlock::default(),
                    rows: 1,
                    columns: frames,
                    placements: vec![[0; PLACEMENT_SIZE]; frames],
                })
                .collect(),
            clips: Vec::new(),
            textures: Vec::new(),
        }
    }

    /// A fight whose points move along x by the game frame.
    struct Moving {
        frame: std::cell::Cell<i32>,
        step: i32,
        hand: [i32; 3],
        /// What a party's scale loses in every game frame, in 4096ths of its start.
        shrinks: i32,
    }

    impl Field for Moving {
        fn point(&self, who: Who, bone: usize, place: [i32; 3]) -> [i32; 3] {
            let far = match who {
                Who::Actor => 0,
                Who::Target(index) => 10_000 * (index as i32 + 1),
            };
            [
                far + self.frame.get() * self.step + place[0],
                place[1] + bone as i32,
                place[2],
            ]
        }
        fn bone(&self, who: Who, bone: usize) -> [i32; 3] {
            self.point(who, bone, [0; 3])
        }
        fn stands(&self, who: Who) -> [i32; 3] {
            self.point(who, 0, [0; 3])
        }
        fn hand(&self) -> [i32; 3] {
            self.hand
        }
        fn ground(&self) -> i32 {
            0
        }
        fn arc(&self, across: i32, _along: i32) -> i16 {
            across as i16
        }
        fn scale(&self, _who: Who, started: Option<[i32; 3]>) -> Option<[i32; 3]> {
            let size = 4096 - self.frame.get() * self.shrinks;
            started.map(|scale| scale.map(|part| part * size / 4096))
        }
    }

    fn field(step: i32) -> Moving {
        Moving {
            frame: std::cell::Cell::new(0),
            step,
            hand: [0, -1000, 0],
            shrinks: 0,
        }
    }

    fn body(points: &[(i16, i16, [i16; 3])]) -> Body {
        Body {
            points: (points.iter())
                .map(|&(kind, bone, place)| Point {
                    kind,
                    bone,
                    place,
                    last: 0,
                })
                .collect(),
            bone_scales: Vec::new(),
            scale: None,
            byte: 0,
        }
    }

    fn windows(pairs: &[(usize, i16, i16)]) -> [[i16; 2]; WINDOWS] {
        let mut windows = [[-1; 2]; WINDOWS];
        for &(index, start, end) in pairs {
            windows[index] = [start, end];
        }
        windows
    }

    /// Passes of a fight whose game frame goes on by one each.
    fn run(
        effects: &mut Effects,
        package: &Package,
        field: &Moving,
        passes: i32,
        loops_end: bool,
    ) -> Vec<Vec<Drawn>> {
        let mut draws = || 0u32;
        (0..passes)
            .map(|_| {
                effects.pass(package, field, &mut draws, loops_end);
                field.frame.set(field.frame.get() + 1);
                effects.drawn().to_vec()
            })
            .collect()
    }

    #[test]
    fn the_table_gives_the_file_and_the_frames_of_an_effect() {
        let base = FILES - 8;
        let mut image = vec![0u8; 8 + 4 * COUNT];
        for number in 0..COUNT {
            let at = 8 + 4 * number;
            image[at..at + 2].copy_from_slice(&(1000 + number as u16).to_le_bytes());
        }
        image[8 + 4 * 5 + 2..8 + 4 * 5 + 4].copy_from_slice(&(-93i16).to_le_bytes());
        let table = table(&image, base).expect("table");
        assert_eq!(table.len(), COUNT);
        assert_eq!(
            entry(&table, 5),
            Some(Entry {
                file: 1005,
                shift: -93
            })
        );
        // A number the table does not have is effect 18.
        assert_eq!(entry(&table, 70).map(|entry| entry.file), Some(1018));
        assert!(super::table(&image[..40], base).is_none());
    }

    #[test]
    fn the_frames_of_an_effect_move_every_window_that_is_there() {
        let moved = shifted(windows(&[(0, 100, 120), (3, 5, 9)]), -10);
        assert_eq!(moved[0], [90, 110]);
        assert_eq!(moved[3], [-5, -1]);
        assert_eq!(moved[1], [-1, -1]);
    }

    #[test]
    fn a_point_is_scaled_by_its_bone_and_its_party() {
        let mut party = body(&[(31, 2, [100, -200, 50]), (20, 31, [-1; 3])]);
        party.points[1].last = -1;
        assert_eq!(party.point(31), Some((2, [100, -200, 50])));
        // A record without a place of its own stands for the kind its second half names.
        assert_eq!(party.point(20), Some((2, [100, -200, 50])));
        assert_eq!(party.point(7), None);
        party.bone_scales = vec![BoneScale::NONE; 3];
        party.bone_scales[2].scale = [2048, 4096, 8192];
        party.scale = Some([4096, 2048, 4096]);
        assert_eq!(party.point(31), Some((2, [50, -100, 100])));
    }

    #[test]
    fn record_i_takes_pair_i_and_a_task_runs_from_its_start_to_its_end() {
        // Record 0 on every target, record 2 on the actor; pair 1 has no record and
        // record 3 no start.
        let package = package(
            &[(0, 3, 1, &[31]), (2, 3, 0, &[20, 21]), (3, 3, 0, &[20])],
            &[(36, 2)],
            &[(191, 3)],
        );
        let actor = body(&[(20, 7, [0; 3])]);
        let targets = [body(&[(31, 1, [0; 3])]), body(&[(31, 1, [0; 3])])];
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 2, 2), (1, 0, 9), (2, 4, 4)]),
            &actor,
            &targets,
            false,
        );
        // Two targets with the point, the actor with one of the two kinds.
        assert_eq!(effects.living(), (3, 0));
        assert!(effects.unread().is_empty());
        let field = field(0);
        let shown = run(&mut effects, &package, &field, 9, false);
        assert!(shown[0].is_empty() && shown[1].is_empty());
        // Game frame 2: routine 3 of both targets, once: three sprites each, and the
        // models whose nodes lie behind their makers' in the same game frame. The
        // first target's task has ended when the second's asks for its models, so the
        // first of those takes its node, in front, and is stepped a game frame later.
        let sprites = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter(|drawn| matches!(drawn, Drawn::Sprite { .. }))
                .count()
        };
        let models = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Model { frame, turn, .. } => Some((*frame, *turn)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(sprites(&shown[2]), 6);
        assert_eq!(
            models(&shown[2]),
            [(0, 0), (0, 1365), (0, 2730), (0, 1365), (0, 2730)]
        );
        assert!(shown[2].contains(&Drawn::Sprite {
            sprite: 0,
            frame: 0,
            place: [20_000, 1, 0],
            angles: [0, 1365, 0],
        }));
        // The sprite has two frames, the model three; the actor's task comes in 4.
        assert_eq!(sprites(&shown[3]), 6);
        assert_eq!(
            models(&shown[3]),
            [(0, 0), (1, 0), (1, 1365), (1, 2730), (1, 1365), (1, 2730)]
        );
        assert_eq!((sprites(&shown[4]), models(&shown[4]).len()), (3, 8));
        assert_eq!((sprites(&shown[5]), models(&shown[5]).len()), (3, 4));
        assert_eq!((sprites(&shown[6]), models(&shown[6]).len()), (0, 3));
        assert_eq!(models(&shown[7]), [(2, 0)]);
        assert!(shown[8].is_empty());
        // The loader waits for the largest end frame, 4, which is over by now.
        assert!(effects.is_over());
    }

    #[test]
    fn a_task_of_an_unread_routine_only_counts_and_is_named() {
        // Every routine of the step has its case in the build; 53 is past its table.
        let package = package(&[(0, 53, 0, &[0])], &[], &[]);
        let mut effects = Effects::default();
        let actor = body(&[(0, 0, [0; 3])]);
        effects.start(&package, windows(&[(0, 1, 3)]), &actor, &[], false);
        assert_eq!(effects.unread(), [53]);
        let field = field(0);
        let shown = run(&mut effects, &package, &field, 3, false);
        assert!(shown.iter().all(Vec::is_empty));
        assert_eq!(effects.living(), (1, 0));
        run(&mut effects, &package, &field, 1, false);
        assert!(effects.is_over());
    }

    #[test]
    fn the_hit_plays_a_sprite_and_a_burst_on_a_target_that_was_not_missed() {
        // Routine 1 on every target at point kind 0, for the one game frame of the
        // hit (the packages of the disc have it on the actor: the test behind this).
        let package = package(&[(0, 1, 1, &[0])], &[(1, 4), (2, 3)], &[]);
        let actor = body(&[(0, 0, [0; 3])]);
        let hit = body(&[(0, 2, [7, 0, 0])]);
        let missed = Body {
            byte: crate::battle::resolve::effect::MISSED,
            ..hit.clone()
        };
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 2, 2)]),
            &actor,
            &[hit, missed],
            false,
        );
        assert!(effects.unread().is_empty());
        assert_eq!(effects.living(), (2, 0));
        let shown = run(&mut effects, &package, &field, 4, false);
        assert!(shown[0].is_empty() && shown[1].is_empty());
        // At the first target's point: the sprite of thing 2 and sixteen pieces of
        // thing 1; nothing at the second target, which was missed.
        let at = [10_007, 2, 0];
        let sprites = |drawn: &[Drawn], wanted: usize| {
            (drawn.iter())
                .filter(|drawn| matches!(drawn, Drawn::Sprite { sprite, .. } if *sprite == wanted))
                .count()
        };
        assert_eq!((sprites(&shown[2], 1), sprites(&shown[2], 0)), (1, 16));
        assert_eq!(shown[2].len(), 17);
        assert!(shown[2].iter().all(|drawn| matches!(
            drawn,
            Drawn::Sprite { place, .. } if place[0] < 20_000
        )));
        assert!(shown[2].contains(&Drawn::Sprite {
            sprite: 1,
            frame: 0,
            place: at,
            angles: [0; 3],
        }));
        // The tasks are gone with the game frame of the hit.
        assert_eq!(effects.living().0, 0);
    }

    #[test]
    fn the_hit_on_the_acting_party_goes_by_the_byte_of_the_first_target() {
        // Record 0 as the packages of the attacks have it on the disc: routine 1 on
        // the actor, at a point of its own body. Its task keeps the byte of the first
        // target, whatever the actor's own is.
        let package = package(&[(0, 1, 0, &[0])], &[(1, 4), (2, 3)], &[]);
        let missed = crate::battle::resolve::effect::MISSED;
        let actor = |byte| Body {
            byte,
            ..body(&[(0, 2, [7, 0, 0])])
        };
        let target = |byte| Body {
            byte,
            ..body(&[(0, 0, [0; 3])])
        };
        let field = field(0);
        let drawn = |actor: Body, targets: &[Body]| {
            let mut effects = Effects::default();
            effects.start(&package, windows(&[(0, 2, 2)]), &actor, targets, false);
            assert_eq!(effects.living(), (1, 0));
            run(&mut effects, &package, &field, 4, false)[2].len()
        };
        assert_eq!(drawn(actor(0), &[target(0)]), 17);
        assert_eq!(drawn(actor(missed), &[target(0)]), 17);
        assert_eq!(drawn(actor(0), &[target(missed)]), 0);
        assert_eq!(drawn(actor(0), &[target(missed), target(0)]), 0);
        assert_eq!(drawn(actor(0), &[target(0), target(missed)]), 17);
        // Without a target the build takes a clear byte.
        assert_eq!(drawn(actor(missed), &[]), 17);
    }

    #[test]
    fn the_main_piece_of_a_spell_plays_a_model_the_clip_and_a_row() {
        // Routine 7 on the target for the one game frame its window is open: model 16
        // of three frames at the target's place, the clip for three game frames, and
        // row 13 held for three.
        let other = package(&[(0, 31, 1, &[0])], &[], &[(16, 3), (26, 2)]);
        let package = package(&[(0, 7, 1, &[0])], &[], &[(16, 3), (26, 2)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 1, 1)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 6, false);
        let model = |model, frame| Drawn::Model {
            model,
            frame,
            place: [10_000, 0, 0],
            turn: 0,
        };
        // The model's task lies behind its maker's, so it is stepped in the pass it
        // is made in.
        assert!(shown[0].is_empty());
        assert_eq!(
            [&shown[1], &shown[2], &shown[3]],
            [&[model(0, 0)], &[model(0, 1)], &[model(0, 2)]]
        );
        assert!(shown[4].is_empty());
        // The row is stopped by the fourth step of its task, three game frames behind
        // its start, and the loader waits for that.
        assert_eq!(
            effects.take_asked(),
            [Asks::Clip(3), Asks::Row(13), Asks::RowStop(13)]
        );
        assert!(effects.is_over());

        // Routine 31: row 15 is started and not held, model 26.
        let mut effects = Effects::default();
        effects.start(&other, windows(&[(0, 0, 0)]), &actor, &[target], false);
        let shown = run(&mut effects, &other, &field, 3, false);
        assert_eq!([&shown[0], &shown[1]], [&[model(1, 0)], &[model(1, 1)]]);
        assert_eq!(effects.take_asked(), [Asks::Row(15), Asks::Clip(2)]);
        assert!(effects.is_over());
    }

    #[test]
    fn the_player_of_the_clip_counts_before_it_picks_a_picture() {
        // Routine 7 asks for the clip for the three frames of its model in pass 1; the
        // clip's task lies behind its maker's and is stepped in that pass.
        let mut package = package(&[(0, 7, 1, &[0])], &[], &[(16, 3)]);
        package.clips = vec![vec![vec![0]; 2]];
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let pictures = |loops_end| {
            let mut effects = Effects::default();
            let starts = windows(&[(0, 1, 1)]);
            effects.start(&package, starts, &actor, &[target.clone()], false);
            let mut draws = || 0u32;
            let shown: Vec<_> = (0..5)
                .map(|_| {
                    effects.pass(&package, &field, &mut draws, loops_end);
                    (effects.picture(), effects.living().0)
                })
                .collect();
            assert!(effects.is_over());
            shown
        };
        // The counts 1 and 2 show the second picture and then the first of the two;
        // the count 3 is the end, without a picture. Behind the step's last pass the
        // model's task and the clip's live.
        assert_eq!(
            pictures(false),
            [(None, 1), (Some(1), 2), (Some(0), 2), (None, 0), (None, 0)]
        );
        // The end of a capture's loops ends the task at its first step.
        assert_eq!(pictures(true)[1], (None, 1));
    }

    #[test]
    fn the_model_of_routine_32_swells_for_as_long_as_the_clip_has_pictures() {
        // Routine 32 on the target for one game frame, with a clip of three pictures:
        // the first mesh object of model 17 at the target's place, larger by seven
        // eighths of the growth before in each game frame, sprite 23 at the point and
        // row 13 held for 53 game frames.
        let mut package = package(&[(0, 32, 1, &[0])], &[(23, 2)], &[(16, 1), (17, 1)]);
        package.clips = vec![vec![Vec::new(); 3]];
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 56, false);
        let swell = |scale| Drawn::Piece {
            model: 1,
            object: 0,
            place: [10_000, 0, 0],
            angles: [0; 3],
            scale,
        };
        let sprite = |frame| Drawn::Sprite {
            sprite: 0,
            frame,
            place: [10_009, 13, 9],
            angles: [0; 3],
        };
        // 0x23F000 times 7 / 8 is 2060800, a size of 503; then 1803200 more, 943;
        // then 1577800 more, 1328.
        assert_eq!(shown[0], [swell(503), sprite(0)]);
        assert_eq!(shown[1], [swell(943), sprite(1)]);
        assert_eq!(shown[2], [swell(1328)]);
        assert!(shown[3].is_empty());
        assert_eq!(
            effects.take_asked(),
            [Asks::Clip(3), Asks::Row(13), Asks::RowStop(13)]
        );
        assert!(effects.is_over());

        // The size never reaches seven times the first growth, 4025 of 4096.
        package.clips = vec![vec![Vec::new(); 200]];
        let mut effects = Effects::default();
        let target = body(&[(0, 4, [9, 9, 9])]);
        effects.start(&package, windows(&[(0, 0, 0)]), &actor, &[target], false);
        let shown = run(&mut effects, &package, &field, 201, false);
        let sizes: Vec<i32> = (shown.iter().flatten())
            .filter_map(|drawn| match drawn {
                Drawn::Piece { scale, .. } => Some(*scale),
                _ => None,
            })
            .collect();
        assert_eq!(sizes.len(), 200);
        assert!(sizes.is_sorted());
        assert_eq!(sizes[199], 4024);
    }

    #[test]
    fn routine_30_swells_a_model_of_two_shapes_and_throws_two_fountains() {
        // Routine 30 on the target for one game frame; every draw is 0, so a piece of
        // a fountain starts with the speeds -31744, 0 and -31744 and its sprite goes
        // on by half a frame a game frame.
        let package = package(&[(0, 30, 1, &[0])], &[(13, 2)], &[(5, 1)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 64, false);
        assert_eq!(effects.take_asked(), [Asks::Clip(100), Asks::Row(19)]);
        let mixed = |share, turn, scale, fade| Drawn::Mixed {
            model: 0,
            share,
            place: [10_000, 0, 0],
            angles: [0, turn, 0],
            scale,
            fade,
        };
        let thrown = |frame, turn, offset| Drawn::Thrown {
            sprite: 0,
            frame,
            place: [10_009, 13, 9],
            angles: [0, turn, 0],
            offset,
        };
        // The model first, then the 42 pieces of the two fountains, all alike here.
        assert_eq!(shown[0].len(), 43);
        assert_eq!(shown[0][0], mixed(2048, 128, 503, 0));
        assert_eq!(shown[0][1], thrown(0, 64, [-28, -1, -28]));
        assert_eq!(shown[0][42], thrown(0, 64, [-28, -1, -28]));
        assert_eq!(shown[1][1], thrown(0, 128, [-54, -2, -54]));
        assert!(matches!(shown[2][1], Drawn::Thrown { frame: 1, .. }));
        assert!(matches!(shown[3][1], Drawn::Thrown { frame: 1, .. }));
        // A piece ends with its sprite; the model lives for 62 game frames, swings
        // between its shapes and fades from the 45th count on.
        assert_eq!(shown[4].len(), 1);
        assert_eq!(shown[2][0], mixed(2048 + 2048, 384, 1328, 0));
        assert!(matches!(shown[44][0], Drawn::Mixed { fade: 0, .. }));
        assert!(matches!(shown[45][0], Drawn::Mixed { fade: 256, .. }));
        assert!(matches!(shown[61][0], Drawn::Mixed { fade: 4352, .. }));
        assert!(shown[62].is_empty());
        // The clip's task counts to 100 from the pass of the case, and the loader
        // waits for it.
        run(&mut effects, &package, &field, 35, false);
        assert!(!effects.is_over());
        run(&mut effects, &package, &field, 1, false);
        assert!(effects.is_over());

        // A party that the action missed, or that refused its status, gets the first
        // fountain alone.
        let mut refused = target;
        refused.byte = REFUSED;
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 0, 0)]), &actor, &[refused], false);
        let shown = run(&mut effects, &package, &field, 1, false);
        assert_eq!(shown[0].len(), 13);
    }

    #[test]
    fn routine_51_plays_a_sound_and_a_model_when_a_layer_of_a_shield_went() {
        // Routine 51 on the target at point kind 31 for one game frame.
        let package = package(&[(2, 51, 1, &[31])], &[], &[(6, 2)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let mut target = body(&[(31, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(2, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 3, false);
        assert!(shown.iter().all(Vec::is_empty));
        assert!(effects.take_asked().is_empty());

        // Each of the three bits of a layer that went starts it.
        for bit in [0x08, 0x10, 0x20] {
            target.byte = bit;
            let mut effects = Effects::default();
            effects.start(
                &package,
                windows(&[(2, 0, 0)]),
                &actor,
                &[target.clone()],
                false,
            );
            field.frame.set(0);
            let shown = run(&mut effects, &package, &field, 3, false);
            let model = |frame| Drawn::Model {
                model: 0,
                frame,
                place: [10_009, 13, 9],
                turn: 0,
            };
            assert_eq!([&shown[0], &shown[1]], [&[model(0)], &[model(1)]]);
            assert!(shown[2].is_empty());
            assert_eq!(effects.take_asked(), [Asks::Sound(73)]);
        }
    }

    #[test]
    fn the_lights_of_routine_20_fly_in_and_drag_their_rings() {
        // Routine 20 on the target for one game frame. The sprite has three frames of
        // one quad, 100, 200 and 300 along x from the place.
        let mut package = package(&[(0, 20, 1, &[0])], &[(24, 3)], &[(19, 1), (18, 2)]);
        let quad = |x: i16| {
            let mut bytes = [0u8; crate::package::QUAD_SIZE];
            bytes[16..18].copy_from_slice(&x.to_le_bytes());
            bytes
        };
        package.sprites[0].frames = vec![vec![quad(100)], vec![quad(200)], vec![quad(300)]];
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 4, false);
        assert_eq!(effects.take_asked(), [Asks::Row(20)]);
        let at = |x| [10_000 + x, 0, 0];
        let light = |frame| Drawn::Sprite {
            sprite: 0,
            frame,
            place: at(0),
            angles: [0; 3],
        };
        let ribbon = |points: [i32; 4]| Drawn::Ribbon {
            model: 0,
            points: points.map(at),
            angles: [0; 3],
        };
        // Six lights with their ribbons, then the model of the party's place; the
        // first light is the one without a turn.
        assert_eq!(shown[0].len(), 13);
        assert_eq!(shown[0][..2], [light(0), ribbon([100; 4])]);
        assert!(matches!(
            shown[0][2],
            Drawn::Sprite {
                angles: [0, 682, 0],
                ..
            }
        ));
        assert_eq!(
            shown[0][12],
            Drawn::Model {
                model: 1,
                frame: 0,
                place: at(0),
                turn: 0
            }
        );
        // The rings move on behind every other game frame.
        assert_eq!(shown[1][..2], [light(1), ribbon([200, 100, 100, 100])]);
        assert_eq!(shown[2][..2], [light(2), ribbon([300, 100, 100, 100])]);
        assert!(shown[3].is_empty());
        assert!(effects.is_over());
    }

    #[test]
    fn the_light_of_the_routines_41_to_43_sheds_pieces_that_fall_and_shrink() {
        // Routine 43 on the target from game frame 2 on; the path has five frames of
        // one quad, 100 further along x each, and every draw is 0, so a piece is a
        // half and is drawn twice.
        let mut package = package(&[(0, 43, 1, &[0])], &[(4, 5), (3, 1), (5, 1)], &[]);
        let quad = |x: i16| {
            let mut bytes = [0u8; crate::package::QUAD_SIZE];
            bytes[16..18].copy_from_slice(&x.to_le_bytes());
            bytes
        };
        package.sprites[0].frames = (1..=5).map(|frame| vec![quad(100 * frame)]).collect();
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, -500, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 2, 2)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 9, false);
        assert_eq!(effects.take_asked(), [Asks::Row(20)]);
        let light = |frame| Drawn::Sprite {
            sprite: 0,
            frame,
            place: [10_009, -496, 9],
            angles: [0; 3],
        };
        let shed = |sprite, x, third: i16, steps: i16, size| Drawn::Shed {
            sprite,
            place: [10_009 + x, -495, 9],
            angles: [0, third * 1365 + steps * 128, 0],
            size,
        };
        // The case plays the sprite; the place sheds from the game frame behind it,
        // three pieces at the first quad of the path's frames 0 and 4. The count was
        // 2 at the case, so the first three show the sprite of an odd count.
        assert_eq!(shown[2], [light(0)]);
        assert_eq!(
            shown[3],
            [
                shed(1, 100, 0, 1, 1984),
                shed(1, 100, 1, 1, 1984),
                shed(1, 100, 2, 1, 1984),
                light(1)
            ]
        );
        // A piece below a half is drawn once more; it has fallen less than a unit.
        // (A place is cut towards zero, so the first 256 of 1024 show as a whole unit.)
        assert_eq!(shown[4][0], shed(1, 100, 0, 2, 1920));
        assert_eq!(shown[4].len(), 4);
        assert_eq!(shown[5], [light(3)]);
        assert_eq!(shown[6], [light(4)]);
        assert_eq!(shown[7][0], shed(2, 500, 0, 1, 1984));
        assert_eq!(shown[7].len(), 3);
        assert_eq!(shown[8].len(), 3);

        // A larger piece falls on until it is under the ground.
        let mut piece = Piece::Falling {
            sprite: 0,
            size: 6143,
            place: [0, -2 << 10, 0],
            fall: 0,
            turn: 0,
        };
        let mut effects = Effects::default();
        let mut draws = || 0u32;
        let mut lived = 0;
        while effects.piece(&mut piece, &package, 0, [0; 3], &mut draws) {
            lived += 1;
        }
        // 767, then 719 + 759, then 1385 + 751 of 1024: under the ground in the third.
        assert_eq!(lived, 2);
        assert_eq!(
            effects.drawn()[2],
            Drawn::Shed {
                sprite: 0,
                place: [0, 2, 0],
                angles: [0, 384, 0],
                size: 5951,
            }
        );
    }

    #[test]
    fn the_routines_13_28_and_49_play_a_model_and_a_fountain() {
        let package = package(
            &[
                (0, 49, 1, &[0]),
                (1, 13, 1, &[0]),
                (2, 28, 0, &[0]),
                (3, 35, 0, &[0]),
            ],
            &[(32, 2), (31, 2)],
            &[(22, 3), (23, 2)],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let starts = |record| windows(&[(record, 0, 0)]);
        let model = |model, place| Drawn::Model {
            model,
            frame: 0,
            place,
            turn: 0,
        };
        let thrown = |shown: &[Drawn], sprite: usize| {
            (shown.iter())
                .filter(|drawn| matches!(drawn, Drawn::Thrown { sprite: of, .. } if *of == sprite))
                .count()
        };

        // Routine 49: the model at the point, and thirty pieces unless refused.
        let mut effects = Effects::default();
        effects.start(&package, starts(0), &actor, &[target.clone()], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 1, false);
        assert_eq!(effects.take_asked(), [Asks::Row(19), Asks::Clip(3)]);
        assert_eq!(shown[0][0], model(0, [10_009, 13, 9]));
        assert_eq!(thrown(&shown[0], 0), 30);
        let mut refused = target.clone();
        refused.byte = REFUSED;
        let mut effects = Effects::default();
        effects.start(&package, starts(0), &actor, &[refused], false);
        let shown = run(&mut effects, &package, &field, 1, false);
        assert_eq!(shown[0], [model(0, [10_009, 13, 9])]);

        // Routine 13: thirty pieces of its own sprite.
        let mut effects = Effects::default();
        effects.start(&package, starts(1), &actor, &[target.clone()], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 1, false);
        assert!(effects.take_asked().is_empty());
        assert_eq!((shown[0].len(), thrown(&shown[0], 1)), (30, 30));

        // The routines 28 and 34 to 36: row 21 and the model at the party's place.
        for record in [2, 3] {
            let mut effects = Effects::default();
            effects.start(&package, starts(record), &actor, &[target.clone()], false);
            assert!(effects.unread().is_empty());
            let shown = run(&mut effects, &package, &field, 1, false);
            assert_eq!(effects.take_asked(), [Asks::Row(21), Asks::Clip(2)]);
            assert_eq!(shown[0], [model(1, [0, 0, 0])]);
        }
    }

    #[test]
    fn the_shape_of_routine_27_grows_stands_whole_and_is_gone() {
        // Routine 27 on the target at point kind 31, from game frame 1 on.
        let package = package(&[(0, 27, 1, &[31])], &[], &[(21, 1)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let mut target = body(&[(31, 4, [9, 9, 9])]);
        target.scale = Some([4096, 2048, 4096]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 1, 1)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 66, false);
        assert_eq!(effects.take_asked(), [Asks::Sound(74), Asks::Clip(64)]);
        let shape = |share| Drawn::Shape {
            model: 0,
            share,
            place: [10_000, 4, 0],
            scale: Some([4096, 2048, 4096]),
            whole: true,
        };
        // The case draws nothing; the shape gains 128 a game frame for 32, stands
        // whole for 31 more and is gone without fading.
        assert!(shown[0].is_empty() && shown[1].is_empty());
        assert_eq!(shown[2], [shape(128)]);
        assert_eq!(shown[3], [shape(256)]);
        assert_eq!(shown[33], [shape(4096)]);
        assert_eq!(shown[64], [shape(4096)]);
        assert!(shown[65].is_empty());
        assert!(effects.is_over());
    }

    #[test]
    fn the_routines_48_and_50_play_a_sprite_and_throw_fountains() {
        let package = package(
            &[(0, 48, 1, &[0]), (1, 50, 1, &[0])],
            &[(27, 5), (26, 2), (30, 7), (29, 2), (28, 2)],
            &[],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let mut refused = target.clone();
        refused.byte = REFUSED;
        let field = field(0);
        let counts = |shown: &[Drawn]| -> Vec<usize> {
            (0..5)
                .map(|wanted| {
                    (shown.iter())
                        .filter(|drawn| match drawn {
                            Drawn::Sprite { sprite, .. } | Drawn::Thrown { sprite, .. } => {
                                *sprite == wanted
                            }
                            _ => false,
                        })
                        .count()
                })
                .collect()
        };
        // (record, the party, the clip's count, the pieces by sprite)
        let cases = [
            (0, &target, 5, [1, 30, 0, 0, 0]),
            (0, &refused, 5, [1, 0, 0, 0, 0]),
            (1, &target, 7, [0, 0, 1, 12, 30]),
            (1, &refused, 7, [0, 0, 1, 12, 0]),
        ];
        for (record, party, clip, pieces) in cases {
            let mut effects = Effects::default();
            let starts = windows(&[(record, 0, 0)]);
            effects.start(&package, starts, &actor, &[party.clone()], false);
            assert!(effects.unread().is_empty());
            let shown = run(&mut effects, &package, &field, 1, false);
            assert_eq!(effects.take_asked(), [Asks::Row(19), Asks::Clip(clip)]);
            assert_eq!(counts(&shown[0]), pieces);
        }
    }

    #[test]
    fn the_routines_38_and_39_play_their_pieces_and_39_ends_with_a_sound() {
        let package = package(
            &[(0, 38, 1, &[0]), (1, 39, 1, &[0])],
            &[(7, 1), (25, 1)],
            &[(1, 1), (2, 1), (20, 1)],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let model = |model, turn| Drawn::Model {
            model,
            frame: 0,
            place: [10_000, 0, 0],
            turn,
        };
        let sprite = |sprite| Drawn::Sprite {
            sprite,
            frame: 0,
            place: [10_009, 13, 9],
            angles: [0; 3],
        };

        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 1, false);
        let sounds = [Asks::Sound(32), Asks::Sound(33), Asks::Sound(34)];
        assert_eq!(effects.take_asked(), sounds);
        assert_eq!(shown[0], [model(0, 0), model(1, 0), sprite(0)]);

        // Routine 39: the test's arc tangent answers the way along x, from the
        // party at 10 000 to the actor at 0. The task ends with its sound in the
        // fourth game frame behind the case, the held sound is stopped in the
        // twenty-first pass.
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(1, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 4, false);
        assert_eq!(shown[0], [model(2, -10_000), sprite(1)]);
        assert_eq!(effects.take_asked(), [Asks::SoundHeld(35)]);
        run(&mut effects, &package, &field, 1, false);
        assert_eq!(effects.take_asked(), [Asks::Sound(36)]);
        assert!(!effects.is_over());
        run(&mut effects, &package, &field, 15, false);
        assert!(effects.take_asked().is_empty());
        run(&mut effects, &package, &field, 1, false);
        assert_eq!(effects.take_asked(), [Asks::SoundStop(35)]);
        assert!(effects.is_over());
    }

    #[test]
    fn the_routines_40_and_47_play_their_pieces_by_a_count_of_game_frames() {
        let package = package(
            &[(0, 40, 1, &[0]), (1, 47, 1, &[0])],
            &[(374, 1), (22, 1), (21, 1)],
            &[(178, 1), (179, 3), (15, 1), (14, 1)],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let turn: i16 = -10_000;
        let at = [10_009, 13, 9];
        let model = |model, place, turn| Drawn::Model {
            model,
            frame: 0,
            place,
            turn,
        };
        let sprite = |sprite, turn| Drawn::Sprite {
            sprite,
            frame: 0,
            place: at,
            angles: [0, turn, 0],
        };

        // Routine 40: the case is pass 0, the count 1 is pass 2, the count 93 pass 94.
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 96, false);
        assert!(shown[0].is_empty() && shown[1].is_empty());
        assert_eq!(shown[2].len(), 10);
        assert_eq!(shown[2][0], model(0, at, turn));
        assert_eq!(shown[2][1], model(0, at, turn + 819));
        assert_eq!(shown[2][4], model(0, at, turn + 3276));
        assert_eq!(shown[2][5], sprite(0, turn));
        assert_eq!(shown[2][9], sprite(0, turn + 3276));
        assert!(shown[3..94].iter().all(Vec::is_empty));
        assert_eq!(shown[94], [model(1, at, turn)]);
        assert_eq!(
            effects.take_asked(),
            [
                Asks::Way(6),
                Asks::Sound(24),
                Asks::Sound(74),
                Asks::WayEnd(6),
                Asks::Sound(26),
                Asks::Clip(3)
            ]
        );

        // Routine 47: the count 0 is pass 1, 48 is pass 49 and 51 pass 52.
        let mut effects = Effects::default();
        field.frame.set(0);
        effects.start(&package, windows(&[(1, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 82, false);
        let party = [10_000, 0, 0];
        assert!(shown[0].is_empty());
        assert_eq!(shown[1].len(), 8);
        assert_eq!(shown[1][0], model(2, party, turn + 1024));
        assert_eq!(shown[1][3], model(2, party, turn.wrapping_add(4096)));
        assert_eq!(shown[1][4], sprite(1, turn));
        assert_eq!(shown[1][7], sprite(1, turn + 3072));
        assert_eq!(shown[49], [sprite(2, turn)]);
        assert_eq!(shown[52], [model(3, party, turn)]);
        assert_eq!(
            effects.take_asked(),
            [
                Asks::Clip(100),
                Asks::SoundHeld(30),
                Asks::SoundHeld(29),
                Asks::SoundStop(30),
                Asks::SoundStop(29)
            ]
        );
        // The clip's task counts to 100 from the pass of the case.
        run(&mut effects, &package, &field, 17, false);
        assert!(!effects.is_over());
        run(&mut effects, &package, &field, 1, false);
        assert!(effects.is_over());
    }

    #[test]
    fn a_summon_plays_its_models_by_the_count_of_the_action() {
        // Routine 19 on the actor from game frame 1 to 13.
        let package = package(
            &[(9, 19, 0, &[0])],
            &[],
            &[(33, 1), (34, 1), (35, 1), (36, 1)],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(9, 1, 13)]), &actor, &[], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 15, false);
        let models = |shown: &[Drawn]| -> Vec<usize> {
            (shown.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Model { model, .. } => Some(*model),
                    _ => None,
                })
                .collect()
        };
        // The first at the start, the third at the counts 3 and 12, the fourth at
        // 6, the second at 12; nothing behind the end.
        let played: Vec<Vec<usize>> = shown.iter().map(|shown| models(shown)).collect();
        let none: Vec<usize> = Vec::new();
        assert_eq!(played[0], none);
        assert_eq!(played[1], [0]);
        assert_eq!(played[2], none);
        assert_eq!(played[3], [2]);
        assert_eq!(played[6], [3]);
        assert_eq!(played[12], [1, 2]);
        assert_eq!(played[13], none);
        assert_eq!(played[14], none);
        assert_eq!(effects.take_asked(), [Asks::Way(6), Asks::WayEnd(6)]);
        assert!(effects.is_over());
    }

    #[test]
    fn the_routines_21_and_52_play_a_sprite_every_game_frame_of_their_window() {
        // Routine 21 from game frame 1 to 2; routine 52 at two points from the
        // start of the action to game frame 3. Every sprite has one frame.
        let package = package(
            &[(7, 21, 0, &[24]), (9, 52, 0, &[415, 416])],
            &[(39, 1), (428, 1)],
            &[],
        );
        let actor = body(&[(24, 0, [0; 3]), (415, 0, [0; 3]), (416, 0, [0; 3])]);
        let field = field(0);
        let count = |shown: &[Drawn], wanted: usize| {
            (shown.iter())
                .filter(|drawn| matches!(drawn, Drawn::Sprite { sprite, .. } if *sprite == wanted))
                .count()
        };

        let mut effects = Effects::default();
        effects.start(&package, windows(&[(7, 1, 2)]), &actor, &[], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 4, false);
        let played: Vec<usize> = shown.iter().map(|shown| count(shown, 0)).collect();
        assert_eq!(played, [0, 1, 1, 0]);
        assert!(effects.take_asked().is_empty());

        // One of the two tasks holds the sound, for the three game frames of the
        // window, and both play their sprite in each of its four.
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(9, 0, 3)]), &actor, &[], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 5, false);
        let played: Vec<usize> = shown.iter().map(|shown| count(shown, 1)).collect();
        assert_eq!(played, [2, 2, 2, 2, 0]);
        assert_eq!(
            effects.take_asked(),
            [Asks::SoundHeld(69), Asks::SoundStop(69)]
        );

        // A window that starts behind the action's first game frame holds none.
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(9, 1, 3)]), &actor, &[], false);
        run(&mut effects, &package, &field, 5, false);
        assert!(effects.take_asked().is_empty());
    }

    #[test]
    fn the_routines_12_and_17_play_a_model_with_the_clip() {
        let package = package(
            &[(0, 17, 0, &[0]), (1, 12, 0, &[0]), (2, 11, 0, &[0])],
            &[],
            &[(28, 4), (177, 2)],
        );
        let actor = body(&[(0, 0, [0; 3])]);
        let field = field(0);
        let model = |model| Drawn::Model {
            model,
            frame: 0,
            place: [0; 3],
            turn: 0,
        };
        // (record, what is asked, the model)
        let cases = [
            (0, vec![Asks::Sound(72), Asks::Clip(4)], 0),
            (1, vec![Asks::Clip(2)], 1),
            (2, vec![Asks::Sound(72), Asks::Clip(4)], 0),
        ];
        for (record, asked, played) in cases {
            let mut effects = Effects::default();
            effects.start(&package, windows(&[(record, 0, 0)]), &actor, &[], false);
            assert!(effects.unread().is_empty());
            let shown = run(&mut effects, &package, &field, 1, false);
            assert_eq!(effects.take_asked(), asked);
            assert_eq!(shown[0], [model(played)]);
        }
    }

    #[test]
    fn routine_23_plays_the_sprite_of_the_half_word_at_twice_the_speed() {
        // Routine 23 at one point from the start of the action to game frame 1; the
        // sprites 20 (the half word 0) and 35 (-1, and 7 to 9) have four frames.
        let package = package(&[(9, 23, 0, &[163])], &[(20, 4), (35, 4)], &[]);
        let actor = body(&[(163, 0, [0; 3])]);
        let field = field(0);
        let frames = |shown: &[Drawn], wanted: usize| -> Vec<usize> {
            (shown.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Sprite { sprite, frame, .. } if *sprite == wanted => Some(*frame),
                    _ => None,
                })
                .collect()
        };
        for (half, sprite) in [(0, Some(0)), (-1, Some(1)), (8, Some(1)), (10, None)] {
            let mut effects = Effects::default().with_half(half);
            effects.start(&package, windows(&[(9, 0, 1)]), &actor, &[], false);
            assert!(effects.unread().is_empty());
            field.frame.set(0);
            let shown = run(&mut effects, &package, &field, 4, false);
            let Some(sprite) = sprite else {
                assert!(shown.iter().all(Vec::is_empty));
                continue;
            };
            // A sprite in each of the two game frames, each two of its frames on a
            // game frame later.
            assert_eq!(frames(&shown[0], sprite), [0]);
            assert_eq!(frames(&shown[1], sprite), [2, 0]);
            assert_eq!(frames(&shown[2], sprite), [2]);
            assert!(shown[3].is_empty());
            assert_eq!(
                effects.take_asked(),
                [Asks::SoundHeld(69), Asks::SoundStop(69)]
            );
        }
    }

    #[test]
    fn routine_24_throws_up_a_fountain_of_the_sprite_of_the_half_word() {
        // Record 1 as the attacks' packages have it: routine 24 on the target at point
        // kind 0, here for two game frames. The sprites 20 (the half word 0) and 35
        // (-1) have two frames.
        let package = package(&[(1, 24, 1, &[0])], &[(20, 2), (35, 2)], &[(192, 2)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let pieces = |shown: &[Drawn], wanted: usize| {
            (shown.iter())
                .filter(|drawn| matches!(drawn, Drawn::Thrown { sprite, .. } if *sprite == wanted))
                .count()
        };
        let mut effects = Effects::default().with_half(0);
        effects.start(
            &package,
            windows(&[(1, 0, 1)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 3, false);
        // Thirty pieces of sprite 20 at the point, and in the second game frame of the
        // window thirty of sprite 35: the task's half word is -1 by then.
        assert_eq!((pieces(&shown[0], 0), pieces(&shown[0], 1)), (30, 0));
        assert!(shown[0].contains(&Drawn::Thrown {
            sprite: 0,
            frame: 0,
            place: [10_009, 13, 9],
            angles: [0, FOUNTAIN_TURN, 0],
            offset: [-28, -1, -28],
        }));
        assert_eq!((pieces(&shown[1], 0), pieces(&shown[1], 1)), (30, 30));
        assert_eq!(shown[1].len(), 60);
        assert_eq!(effects.take_asked(), [Asks::Row(19), Asks::Row(19)]);
        // The two lights hold their nodes for 33 game frames each.
        assert_eq!(effects.living().0, 2);
        run(&mut effects, &package, &field, 30, false);
        assert_eq!(effects.living(), (1, 0));
        run(&mut effects, &package, &field, 1, false);
        assert!(effects.is_over());

        // A party that was missed or refused shows nothing.
        let refused = Body {
            byte: REFUSED,
            ..target.clone()
        };
        let mut effects = Effects::default().with_half(0);
        effects.start(&package, windows(&[(1, 0, 0)]), &actor, &[refused], false);
        let shown = run(&mut effects, &package, &field, 2, false);
        assert!(shown.iter().all(Vec::is_empty));
        assert!(effects.take_asked().is_empty() && effects.is_over());

        // The half word 10 plays the model alone, with neither a row nor a light.
        let mut effects = Effects::default().with_half(10);
        effects.start(&package, windows(&[(1, 0, 0)]), &actor, &[target], false);
        let shown = run(&mut effects, &package, &field, 3, false);
        let model = |frame| Drawn::Model {
            model: 0,
            frame,
            place: [10_009, 13, 9],
            turn: 0,
        };
        assert_eq!([&shown[0], &shown[1]], [&[model(0)], &[model(1)]]);
        assert!(effects.take_asked().is_empty() && effects.is_over());
    }

    #[test]
    fn the_pieces_of_two_half_words_of_routine_24_are_drawn_to_the_actor() {
        // The half word 8: thirty pieces of sprite 35 that start at the target's point
        // and head for the actor's bone 2, which the test's field has at (0, 2, 0).
        let package = package(&[(1, 24, 1, &[0])], &[(35, 2)], &[(192, 2)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 0, [0; 3])]);
        let field = field(0);
        let mut effects = Effects::default().with_half(8);
        effects.start(
            &package,
            windows(&[(1, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        let shown = run(&mut effects, &package, &field, 5, false);
        let piece = |frame, place| Drawn::Thrown {
            sprite: 0,
            frame,
            place,
            angles: [0; 3],
            offset: [0; 3],
        };
        // Every draw is 0: the speed is -63 units along x and z, of which a 32nd is
        // lost, and 32 1024ths are gained for each of the 10,000 units to the bone.
        assert_eq!(shown[0], vec![piece(0, [9627, 0, -61]); 30]);
        // -382,496 * 31 / 32 less 9627 * 32, and -62,496 * 31 / 32 plus 61 * 32.
        assert_eq!(shown[1], vec![piece(0, [8965, 0, -118]); 30]);
        // The sprite goes on by half a frame a game frame and is gone with its last.
        assert!(matches!(shown[2][0], Drawn::Thrown { frame: 1, .. }));
        assert_eq!((shown[3].len(), shown[4].len()), (30, 0));
        assert_eq!(effects.take_asked(), [Asks::Row(19)]);

        // The half word 9 plays the model with them.
        let mut effects = Effects::default().with_half(9);
        effects.start(&package, windows(&[(1, 0, 0)]), &actor, &[target], false);
        let shown = run(&mut effects, &package, &field, 1, false);
        assert_eq!(shown[0].len(), 31);
        assert!(matches!(shown[0][0], Drawn::Model { frame: 0, .. }));
    }

    #[test]
    fn the_half_word_names_what_routine_24_shows() {
        let hits = [0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9].map(hit_of_half);
        let sprites = [35, 20, 8, 17, 34, 26, 28, 32, 14, 35, 35].map(Some);
        assert_eq!(hits.map(|hit| hit.and_then(|hit| hit.sprite)), sprites);
        let lights = [0, 1, 4, 3, 2, 4, 4, 4, 4, 4, 2].map(Some);
        assert_eq!(hits.map(|hit| hit.map(|hit| hit.light)), lights);
        let drawn_in = hits.map(|hit| hit.is_some_and(|hit| hit.drawn_in));
        assert_eq!(drawn_in[..9], [false; 9]);
        assert_eq!(drawn_in[9..], [true; 2]);
        assert!(
            hits[..10]
                .iter()
                .all(|hit| hit.is_some_and(|hit| !hit.model))
        );
        assert!(hits[10].is_some_and(|hit| hit.model));
        // The byte 10 plays the model alone; nothing past it.
        let alone = hit_of_half(10).expect("the byte 10");
        assert_eq!((alone.sprite, alone.model), (None, true));
        assert_eq!([11, 0x80, 0xFE].map(hit_of_half), [None; 3]);
    }

    #[test]
    fn a_body_attack_leaves_its_half_word_and_the_half_word_names_a_sprite() {
        let halves = [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 16, 37].map(half_of_body_attack);
        assert_eq!(halves, [2, 3, 0, 1, 4, 5, 6, 10, 8, 9, -1, 6, 3]);
        let sprites = [0xFF, 0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 0x80].map(sprite_of_half);
        let named = [35, 20, 8, 17, 34, 26, 28, 32, 35, 35].map(Some);
        assert_eq!(sprites[..10], named);
        assert_eq!(sprites[10..], [None, None]);
    }

    #[test]
    fn the_pieces_of_routine_6_close_in_and_break_into_shards() {
        // Routine 6 on the target for one game frame; every draw is 0, so a piece is
        // half its size and level with the place, and the test's arc tangent answers
        // its first argument.
        let package = package(&[(0, 6, 1, &[0])], &[(33, 2)], &[(24, 1), (25, 1)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 1, 1)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 33, false);
        let piece = |place, turn| Drawn::Piece {
            model: 0,
            object: 0,
            place,
            angles: [0, 0, turn],
            scale: 2048,
        };
        // Eight pieces 300 from the place from the pass of the case on; the first
        // stands along z and heads back along it.
        assert!(shown[0].is_empty());
        assert_eq!(shown[1].len(), 8);
        assert_eq!(shown[1][0], piece([10_000, 0, 300], 170));
        assert!(shown[1].contains(&Drawn::Piece {
            model: 0,
            object: 0,
            place: [10_300, 0, 0],
            angles: [0, -75, 170],
            scale: 2048,
        }));
        // They stand for 26 game frames and close in over four.
        assert_eq!(shown[26][0], piece([10_000, 0, 300], 170 * 26));
        assert_eq!(shown[27][0], piece([10_000, 0, 225], 170 * 27));
        assert_eq!(shown[30][0], piece([10_000, 0, 0], 170 * 30));
        assert_eq!(shown[31][0], piece([10_000, 0, 0], 170 * 31));
        // The break comes with the thirtieth step of the counting task: the sound,
        // the sprite and 24 shards, each stepped in that pass.
        assert_eq!(shown[29].len(), 8);
        assert_eq!(shown[30].len(), 8 + 24 + 1);
        assert!(shown[30].contains(&Drawn::Piece {
            model: 1,
            object: 0,
            place: [10_000, -23, 27],
            angles: [0, 0, 256],
            scale: 2048,
        }));
        let eighth = |drawn: &&Drawn| {
            matches!(
                drawn,
                Drawn::Piece {
                    model: 1,
                    object: 7,
                    ..
                }
            )
        };
        assert_eq!(shown[30].iter().filter(eighth).count(), 3);
        assert!(shown[30].contains(&Drawn::Sprite {
            sprite: 0,
            frame: 0,
            place: [10_000, 0, 0],
            angles: [0; 3],
        }));
        assert_eq!(shown[32].len(), 24);
        assert_eq!(
            effects.take_asked(),
            [
                Asks::Row(14),
                Asks::Clip(PIECES_CLIP),
                Asks::Sound(SHARD_SOUND)
            ]
        );
        // A shard falls back to the ground and ends there when it is slow.
        assert!(!effects.is_over());
        run(&mut effects, &package, &field, 400, false);
        assert!(effects.is_over());
    }

    #[test]
    fn the_routines_8_and_9_play_sprites_and_models_at_the_target() {
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 0, 0])]);
        let field = field(0);
        // Routine 8: the sprites 10 and 9 at the point, the clip for sprite 10's
        // frames.
        let other = package(&[(0, 9, 1, &[0])], &[(18, 2)], &[(11, 2)]);
        let package = package(&[(0, 8, 1, &[0])], &[(9, 2), (10, 3)], &[]);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(0, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 4, false);
        let sprite = |sprite, frame, turn| Drawn::Sprite {
            sprite,
            frame,
            place: [10_009, 4, 0],
            angles: [0, turn, 0],
        };
        assert_eq!(shown[0], [sprite(1, 0, 0), sprite(0, 0, 0)]);
        assert_eq!(shown[2], [sprite(1, 2, 0)]);
        assert_eq!(effects.take_asked(), [Asks::Row(16), Asks::Clip(3)]);
        assert!(effects.is_over());

        // Routine 9: sprite 18 at the point and model 11 at the party's place, five
        // times round.
        let package = other;
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(0, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &package, &field, 3, false);
        let turns = [0, 819, 1638, 2457, 3276];
        for turn in turns {
            assert!(shown[0].contains(&sprite(0, 0, turn)));
            assert!(shown[0].contains(&Drawn::Model {
                model: 0,
                frame: 0,
                place: [10_000, 0, 0],
                turn,
            }));
        }
        assert_eq!(shown[0].len(), 10);
        assert_eq!(effects.take_asked(), [Asks::Row(22), Asks::Clip(2)]);
        assert!(effects.is_over());
    }

    #[test]
    fn the_routines_29_and_33_play_their_pieces_at_the_target() {
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 0, 0])]);
        let field = field(0);
        let at = [10_009, 4, 0];
        let model = |model| Drawn::Model {
            model,
            frame: 0,
            place: [10_000, 0, 0],
            turn: 0,
        };
        // Routine 29: the sprites 12 and 11 at the point, the clip for sprite 12's
        // frames, model 4 at the party's place.
        let first = package(&[(0, 29, 1, &[0])], &[(11, 2), (12, 3)], &[(4, 2)]);
        let mut effects = Effects::default();
        effects.start(
            &first,
            windows(&[(0, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &first, &field, 4, false);
        let sprite = |sprite| Drawn::Sprite {
            sprite,
            frame: 0,
            place: at,
            angles: [0; 3],
        };
        assert_eq!(shown[0], [model(0), sprite(1), sprite(0)]);
        assert_eq!(effects.take_asked(), [Asks::Row(18), Asks::Clip(3)]);
        assert!(effects.is_over());

        // Routine 33: sixteen pieces of sprite 19 from the point, the models 12 and
        // 13 at the party's place.
        let second = package(&[(0, 33, 1, &[0])], &[(19, 2)], &[(12, 1), (13, 1)]);
        let mut effects = Effects::default();
        effects.start(&second, windows(&[(0, 0, 0)]), &actor, &[target], false);
        assert!(effects.unread().is_empty());
        let shown = run(&mut effects, &second, &field, 1, false);
        assert_eq!(shown[0][..2], [model(0), model(1)]);
        assert_eq!(shown[0].len(), 2 + 16);
        assert!(
            shown[0][2..]
                .iter()
                .all(|drawn| matches!(drawn, Drawn::Sprite { sprite: 0, .. }))
        );
        assert_eq!(effects.take_asked(), [Asks::Row(23)]);
    }

    #[test]
    fn the_trail_leaves_a_sprite_for_every_128_units() {
        let package = package(&[(6, 0, 0, &[37])], &[(0, 1)], &[]);
        let mut effects = Effects::default();
        let actor = body(&[(37, 0, [0; 3])]);
        effects.start(&package, windows(&[(6, 0, 4)]), &actor, &[], false);
        // 300 units a game frame along x.
        let field = field(300);
        let shown = run(&mut effects, &package, &field, 4, false);
        let places = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Sprite { place, .. } => Some(place[0]),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        // Nothing in the task's first game frame; then 300 units: two sprites, 44 left.
        assert!(shown[0].is_empty());
        assert_eq!(places(&shown[1]), [0, 150]);
        // 344: two sprites, 88 left; 388: three, 4 left.
        assert_eq!(places(&shown[2]), [300, 450]);
        assert_eq!(places(&shown[3]), [600, 700, 800]);
    }

    #[test]
    fn the_cast_plays_its_model_only_when_its_window_starts_with_the_action() {
        let package = package(&[(7, 18, 0, &[0])], &[], &[(0, 2)]);
        let actor = body(&[(0, 3, [5, 5, 5])]);
        let field = field(0);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(7, 0, 2)]), &actor, &[], false);
        let shown = run(&mut effects, &package, &field, 3, false);
        // At the fighter's place, not at the point's.
        let model = |frame| Drawn::Model {
            model: 0,
            frame,
            place: [0; 3],
            turn: 0,
        };
        assert_eq!(shown[0], [model(0)]);
        assert_eq!(shown[1], [model(1)]);
        assert_eq!(effects.take_asked(), [Asks::Way(6), Asks::WayEnd(6)]);

        let mut effects = Effects::default();
        effects.start(&package, windows(&[(7, 1, 2)]), &actor, &[], false);
        let shown = run(&mut effects, &package, &field, 3, false);
        assert!(shown.iter().all(Vec::is_empty));
    }

    /// The capture's records as file 1060 has them.
    fn capture() -> Package {
        package(
            &[
                (0, 3, 1, &[31]),
                (1, 4, 1, &[31]),
                (2, 25, 0, &[20]),
                (6, 0, 0, &[37, 41]),
                (7, 26, 1, &[0]),
            ],
            &[(0, 1), (36, 1), (37, 8), (38, 1)],
            &[(30, 31), (31, 1), (191, 1)],
        )
    }

    fn shares(drawn: &[Drawn], shell: bool) -> Vec<i32> {
        (drawn.iter())
            .filter_map(|drawn| match drawn {
                Drawn::Shape { share, whole, .. } if *whole == shell => Some(*share),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_light_and_the_rings_of_a_capture_live_until_the_loops_end() {
        let package = capture();
        let actor = body(&[(20, 7, [1, 2, 3])]);
        let targets = [body(&[(0, 0, [0; 3]), (31, 4, [0; 3])])];
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(2, 3, 3), (7, 5, 60)]),
            &actor,
            &targets,
            true,
        );
        let field = field(0);
        let shown = run(&mut effects, &package, &field, 40, false);
        // The case of routine 25 runs in game frame 3; its own routine draws from the
        // next on, whatever the end of the window says.
        assert!(shown[3].is_empty());
        let glows = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Glow { place, count, .. } => Some((*place, *count)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(glows(&shown[4]), [([1, 2 + 7, 3], 1)]);
        assert_eq!(glows(&shown[39]), [([1, 2 + 7, 3], 36)]);
        // Routine 26 starts in game frame 5; its routine counts from 6 and plays model
        // 30 at the target's place in every tenth game frame: 15, 25, 35.
        let rings = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Model { frame, place, .. } => Some((*frame, place[0])),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(rings(&shown[14]).is_empty());
        assert_eq!(rings(&shown[15]), [(0, 10_000)]);
        assert_eq!(rings(&shown[26]), [(11, 10_000), (1, 10_000)]);
        let asked = effects.take_asked();
        assert_eq!(asked, [Asks::Sound(80), Asks::Clip(600), Asks::Way(6)]);
        // The loops end: the light is gone at once, the rings play out.
        let shown = run(&mut effects, &package, &field, 32, true);
        assert!(glows(&shown[0]).is_empty());
        assert_eq!(effects.take_asked(), [Asks::WayEnd(6)]);
        assert!(!rings(&shown[0]).is_empty());
        assert!(shown[31].is_empty());
        assert!(effects.is_over());
    }

    #[test]
    fn the_shell_grows_in_sixteen_game_frames_and_fades_as_it_came() {
        let package = capture();
        let mut target = body(&[(31, 4, [9, 9, 9])]);
        target.scale = Some([2048; 3]);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(1, 2, 2)]),
            &body(&[]),
            &[target],
            false,
        );
        let field = field(0);
        let shown = run(&mut effects, &package, &field, 30, false);
        assert!(shares(&shown[1], true).is_empty());
        // The shell's node lies behind its maker's: it is stepped in the game frame of
        // the case, at the bone's place (without the point) and the target's scale.
        assert_eq!(
            shown[2],
            [Drawn::Shape {
                model: 0,
                share: 256,
                place: [10_000, 4, 0],
                scale: Some([2048; 3]),
                whole: true,
            }]
        );
        assert_eq!(shares(&shown[17], true), [4096]);
        assert_eq!(shares(&shown[29], true), [4096]);
        assert_eq!(effects.take_asked(), [Asks::Row(9)]);

        // The loops end: whole in the first game frame, with the broken shell at the
        // player's hand from the next on (its node lies in front), gone with share 0.
        let shown = run(&mut effects, &package, &field, 20, true);
        assert_eq!(shares(&shown[0], true), [4096]);
        assert_eq!(effects.take_asked(), [Asks::Sound(63)]);
        assert_eq!(shares(&shown[1], true), [3840]);
        assert_eq!(shares(&shown[16], true), [0]);
        assert!(shares(&shown[17], true).is_empty());
        assert_eq!(shares(&shown[0], false), [] as [i32; 0]);
        assert_eq!(shares(&shown[1], false), [3840]);
        assert_eq!(shares(&shown[15], false), [256]);
        assert!(shares(&shown[16], false).is_empty());
        assert!(shown[1].contains(&Drawn::Shape {
            model: 1,
            share: 3840,
            place: [0, -1000, 0],
            scale: None,
            whole: false,
        }));
        assert!(effects.is_over());
    }

    #[test]
    fn the_shell_has_the_scale_its_target_has_in_the_game_frame() {
        let package = capture();
        let mut target = body(&[(31, 4, [9, 9, 9])]);
        target.scale = Some([4096; 3]);
        let mut effects = Effects::default();
        effects.start(
            &package,
            windows(&[(1, 2, 2)]),
            &body(&[]),
            &[target],
            false,
        );
        let field = Moving {
            shrinks: 100,
            ..field(0)
        };
        let shown = run(&mut effects, &package, &field, 6, false);
        let scales = |drawn: &[Drawn]| match drawn {
            [Drawn::Shape { scale, .. }] => *scale,
            _ => None,
        };
        let first = scales(&shown[2]).expect("the shell");
        let later = scales(&shown[5]).expect("the shell");
        assert_eq!(later, first.map(|part| part - 300));
    }

    #[test]
    fn the_light_of_the_shell_stands_where_the_node_was_left() {
        let package = capture();
        let start = |effects: &mut Effects| {
            let target = body(&[(31, 4, [9, 9, 9])]);
            effects.start(
                &package,
                windows(&[(1, 2, 2)]),
                &body(&[]),
                &[target],
                false,
            );
        };
        let field = field(0);
        // The step's task has node 0 and the shell node 1, of which the build knows
        // nothing: no light.
        let mut effects = Effects::default();
        start(&mut effects);
        run(&mut effects, &package, &field, 20, false);
        assert_eq!(effects.light(), None);
        // Node 0 keeps the place of the step's point, which has ended.
        assert_eq!(effects.kept()[0], Some([10_009, 13, 9]));

        // With a place an effect before has left in node 1 the light stands there, as
        // far as 1500, and grows with the shell to 32 times its bytes.
        let mut kept = Kept::default();
        kept[1] = Some([5, 6, 7]);
        let mut effects = Effects::default().with_kept(kept);
        start(&mut effects);
        run(&mut effects, &package, &field, 3, false);
        let light = |colour| {
            Some(Movable {
                place: [5, 6, 7],
                colour,
                range: LIGHT_RANGE,
            })
        };
        assert_eq!(effects.light(), light([464, 464, 256]));
        run(&mut effects, &package, &field, 20, false);
        assert_eq!(effects.light(), light([7424, 7424, 4096]));
        // It fades with the shell and is put out with it.
        run(&mut effects, &package, &field, 2, true);
        assert_eq!(effects.light(), light([6960, 6960, 3840]));
        run(&mut effects, &package, &field, 14, true);
        assert_eq!(effects.light(), light([464, 464, 256]));
        run(&mut effects, &package, &field, 1, true);
        assert_eq!(effects.light(), None);
        assert!(effects.is_over());
    }

    #[test]
    fn the_lights_of_the_routines_24_and_32_dim_and_are_put_out() {
        // Routine 24 with the half word 0: colour record 1 at the point, 32 times its
        // bytes in the first game frame and a 32nd less in each one behind it.
        let package_24 = package(&[(1, 24, 1, &[0])], &[(20, 2)], &[(192, 2)]);
        let actor = body(&[(0, 0, [0; 3])]);
        let target = body(&[(0, 4, [9, 9, 9])]);
        let field = field(0);
        let mut effects = Effects::default().with_half(0);
        effects.start(
            &package_24,
            windows(&[(1, 0, 0)]),
            &actor,
            &[target.clone()],
            false,
        );
        let light = |place, colour| {
            Some(Movable {
                place,
                colour,
                range: LIGHT_RANGE,
            })
        };
        let at = [10_009, 13, 9];
        run(&mut effects, &package_24, &field, 1, false);
        assert_eq!(effects.light(), light(at, [8160, 0, 3424]));
        run(&mut effects, &package_24, &field, 1, false);
        assert_eq!(effects.light(), light(at, [7905, 0, 3317]));
        run(&mut effects, &package_24, &field, 30, false);
        assert_eq!(effects.light(), light(at, [255, 0, 107]));
        run(&mut effects, &package_24, &field, 1, false);
        assert_eq!(effects.light(), None);
        // The light's node keeps its place.
        assert_eq!(effects.kept()[1], Some(at));

        // Routine 32 with a clip of four pictures: an orange light at the party's
        // place, whole in the first game frame and a quarter less in each.
        let mut package_32 = package(&[(0, 32, 1, &[0])], &[(23, 2)], &[(16, 1), (17, 1)]);
        package_32.clips = vec![vec![Vec::new(); 4]];
        let mut effects = Effects::default();
        effects.start(&package_32, windows(&[(0, 0, 0)]), &actor, &[target], false);
        let at = [10_000, 0, 0];
        run(&mut effects, &package_32, &field, 1, false);
        assert_eq!(effects.light(), light(at, [8160, 4096, 0]));
        run(&mut effects, &package_32, &field, 1, false);
        assert_eq!(effects.light(), light(at, [6120, 3072, 0]));
        run(&mut effects, &package_32, &field, 2, false);
        assert_eq!(effects.light(), light(at, [2040, 1024, 0]));
        run(&mut effects, &package_32, &field, 1, false);
        assert_eq!(effects.light(), None);
    }

    #[test]
    fn a_shell_that_had_not_grown_whole_has_no_follow_up() {
        let package = capture();
        let target = body(&[(31, 4, [0; 3])]);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(1, 0, 0)]), &body(&[]), &[target], true);
        let field = field(0);
        run(&mut effects, &package, &field, 5, false);
        let shown = run(&mut effects, &package, &field, 8, true);
        assert_eq!(shares(&shown[0], true), [1280]);
        assert_eq!(shares(&shown[5], true), [0]);
        assert!(shown[6].is_empty());
        assert_eq!(effects.take_asked(), [Asks::Row(9)]);
        assert!(effects.is_over());
    }

    #[test]
    fn the_spark_of_a_caught_creature_flies_to_the_hand_and_bursts() {
        let package = capture();
        let target = body(&[(31, 0, [0; 3])]);
        let mut effects = Effects::default();
        effects.start(&package, windows(&[(1, 0, 0)]), &body(&[]), &[target], true);
        let mut field = field(0);
        // The hand is 2000 units from the shell, along x.
        field.hand = [12_000, 0, 0];
        run(&mut effects, &package, &field, 16, false);
        let mut draws = {
            let mut next = 0u32;
            move || {
                next += 1;
                next
            }
        };
        let mut seen = Vec::new();
        for _ in 0..200 {
            effects.pass(&package, &field, &mut draws, true);
            seen.push(effects.drawn().to_vec());
            if effects.is_over() {
                break;
            }
        }
        assert_eq!(effects.take_asked(), [Asks::Row(9), Asks::Sound(71)]);
        // The spark's node lies in front of the shell's: it is first stepped a game
        // frame later, at the shell's place, and leaves sprite 37 there.
        let sparks = |drawn: &[Drawn]| {
            (drawn.iter())
                .filter_map(|drawn| match drawn {
                    Drawn::Sprite {
                        sprite: 2,
                        frame,
                        place,
                        ..
                    } => Some((*frame, place[0])),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(sparks(&seen[0]).is_empty());
        assert_eq!(sparks(&seen[1]), [(0, 10_000)]);
        // It gains (2000 * 1024 / 70) * 31 / 32 a game frame at first: 27 units.
        assert_eq!(sparks(&seen[2]), [(1, 10_000), (0, 10_027)]);
        // The sprite has eight frames: a tail of eight.
        assert_eq!(sparks(&seen[9]).len(), 8);
        // It ends in a burst of sixteen within 150 units of the hand.
        let burst = (seen.iter())
            .position(|drawn| sparks(drawn).len() >= 16)
            .expect("a burst");
        let near = sparks(&seen[burst]);
        assert!(near.iter().any(|&(_, x)| (11_850..=12_150).contains(&x)));
        assert!(effects.is_over());
        assert!(seen.len() < 200);
    }

    #[test]
    fn a_piece_of_a_burst_slows_falls_and_turns_round_on_the_ground() {
        let package = package(&[], &[(37, 100)], &[]);
        let mut effects = Effects::default();
        // The draws 63, 0, 31 for the speed: 32, -31 and 0 units a game frame at first;
        // the fourth, 4094, for 4095 of 4096 of a frame a game frame.
        let mut values = [63u32, 0, 31, 4094].into_iter().cycle();
        let mut draws = move || values.next().unwrap_or(0);
        effects.burst(&package, 37, [0, -5, 0], &mut draws);
        assert_eq!(effects.living(), (0, 16));
        let field = field(0);
        effects.pass(&package, &field, &mut draws, false);
        // 32768 * 15 / 16 = 30720: 30 units; -31744 * 15 / 16 + 512 = -29248: -28.
        let Drawn::Sprite { place, frame, .. } = effects.drawn()[0] else {
            panic!("a sprite");
        };
        assert_eq!((place, frame), ([30, -33, 0], 0));
        // It rises ever more slowly, falls and is put on the ground.
        let mut lowest = place[1];
        let mut landed = false;
        for _ in 0..95 {
            effects.pass(&package, &field, &mut draws, false);
            let Some(Drawn::Sprite { place, .. }) = effects.drawn().first().copied() else {
                break;
            };
            lowest = lowest.min(place[1]);
            assert!(place[1] <= 0);
            landed |= place[1] == 0;
        }
        assert!(lowest < -100 && landed);
    }

    #[test]
    fn a_shape_is_the_second_at_no_share_and_the_first_at_the_whole() {
        let (first, second) = ([100, -40, 8], [0, 40, -8]);
        assert_eq!(mix(first, second, 0), second);
        assert_eq!(mix(first, second, SHARE_ONE), first);
        assert_eq!(mix(first, second, SHARE_ONE / 2), [50, 0, 0]);
        // The share of the difference is shifted down, so a way back is rounded away
        // from the first shape: 3 of 4096 of -80 is -1, not 0.
        assert_eq!(mix(first, second, 3), [0, 39, -8]);
        // No scale is handed to the mesh drawer for a target that has none.
        assert_eq!(shell_scale(None), [4096; 3]);
        assert_eq!(shell_scale(Some([2048, 4096, 8192])), [3072, 6144, 12288]);
    }

    #[test]
    fn the_rings_of_the_flare_come_with_the_distance_of_the_light() {
        // A table made for the test: 32768 over the root of the number, so 4096 for 64.
        let steps = (64..256).map(|number| (32768.0 / f64::from(number).sqrt()).round() as i16);
        let unit = Unit::of(steps.collect());
        // A way of 512 has the squares' sum 2 to the 18th: the number 64, shifted
        // down by 9, so exactly 4096. One of 500 has the number 244 and comes to 4097.
        assert_eq!(unit.way([0, 0, 512]), Some([0, 0, 4096]));
        assert_eq!(unit.way([0, 0, 500]), Some([0, 0, 4097]));
        assert_eq!(unit.way([-3, 0, 4]), Some([-2458, 0, 3277]));
        assert_eq!(unit.way([0; 3]), None);
        let flare = |light, eye, looks, distance| flare(light, eye, looks, distance, &unit);
        // The camera looks along z; the light stands 1000 in front of it and 200 aside.
        let (eye, looks, light) = ([0, 0, 0], [0, 0, 512], [200, 0, 1000]);
        // The projection distance is the light's own: no ring.
        assert_eq!(flare(light, eye, looks, 1000), []);
        // Nearer than the light: the ring at C, which lies half of s the way he looks.
        let rings = flare(light, eye, looks, 800);
        assert_eq!(
            rings,
            [Ring {
                place: [0, 0, 500],
                size: 1024
            }]
        );
        // Below three quarters of s two more, below half of it all four.
        assert_eq!(flare(light, eye, looks, 700).len(), 3);
        let rings = flare(light, eye, looks, 400);
        assert_eq!(
            rings.iter().map(|ring| ring.size).collect::<Vec<_>>(),
            [1024, 4096, 2048, 8192]
        );
        assert_eq!(rings[1].place, [100, 0, 750]);
        assert_eq!(rings[2].place, [-100, 0, 250]);
        assert_eq!(rings[3].place, [-50, 0, 375]);
        assert_eq!(flare(light, eye, eye, 400), []);
    }

    #[test]
    fn a_request_names_its_effect_by_its_kind() {
        let spell = |shown: u8| Spell {
            word: 0,
            cost: 0,
            shown,
            need: 0,
            element: 0,
            effect: crate::battle::Descriptor::parse(&[0; 12]).expect("a descriptor"),
        };
        let spells = [spell(25), spell(25), spell(31)];
        assert_eq!(of_request(0, 0, &spells), Some(19));
        assert_eq!(of_request(1, 2, &spells), Some(31));
        assert_eq!(of_request(1, 30, &spells), Some(26));
        assert_eq!(of_request(2, 0, &spells), Some(58));
        assert_eq!(of_request(3, 0, &spells), Some(57));
        assert_eq!(of_request(9, 0, &spells), None);
        // A master's swap: the one that goes has the leave's, the one that comes the
        // summon's, whichever kind the two runners have.
        assert_eq!(of_swap(21, false), Some(57));
        assert_eq!(of_swap(21, true), Some(58));
        assert_eq!(of_swap(22, true), Some(57));
        assert_eq!(of_swap(22, false), Some(58));
        assert_eq!(of_swap(2, false), None);
    }

    #[test]
    fn a_body_attack_names_its_effect_by_slot_and_attack() {
        // By the slot: 38 plus the number the attack is played by, 30 from slot 4 on.
        assert_eq!(
            [0, 1, 2, 3, 4, 9].map(|slot| of_body_attack(slot, 0)),
            [41, 38, 40, 39, 30, 30].map(Some)
        );
        // The attacks with an effect of their own, whatever the slot.
        let own = [16, 17, 18, 21, 24, 33, 34, 37, 38].map(|attack| of_body_attack(2, attack));
        assert_eq!(own, [38, 60, 59, 30, 34, 25, 63, 66, 52].map(Some));
        assert_eq!(of_body_attack(2, 28), None);
        assert_eq!(of_body_attack(2, 39), Some(40));
    }

    #[test]
    fn the_players_attack_and_item_name_their_effects() {
        // A synthetic image: three weapon records are not enough, so every table is
        // laid out whole, the records with the values 0, 2, 1, 0, 0, ...
        use crate::model::{WEAPON_ITEMS, WEAPON_RECORD, WEAPON_RECORDS, WEAPONS};
        let base = ATTACKS;
        let mut image = vec![0u8; (WEAPON_RECORDS - base) as usize + WEAPON_ITEMS * WEAPON_RECORD];
        for (value, word) in (0..WEAPONS).zip([7i32, 8, 9]) {
            image[4 * value..4 * value + 4].copy_from_slice(&word.to_le_bytes());
        }
        let items = (ITEMS - base) as usize;
        image[items + 5] = 50;
        let records = (WEAPON_RECORDS - base) as usize;
        image[records + WEAPON_RECORD + 2] = 2;
        image[records + 2 * WEAPON_RECORD + 2] = 1;
        let numbers = Numbers::read(&image, base).expect("the tables");
        assert_eq!([0, 1, 2].map(|w| numbers.of_attack(w)), [7, 9, 8].map(Some));
        // Without a weapon of the table there is no number.
        assert_eq!(numbers.of_attack(255), None);
        assert_eq!(
            [5, 6, 42, 43].map(|item| numbers.of_item(item)),
            [50, 0, 0, 34]
        );
        assert_eq!(Numbers::read(&image[..records], base), None);
        // The half word a weapon leaves for the effects 23 and 24.
        assert_eq!(
            [0, 2, 14, 21].map(weapon_half_word),
            [None, Some(3), Some(0), Some(7)]
        );
    }

    #[test]
    fn the_grades_0_and_2_of_a_capture_catch() {
        assert_eq!([0, 1, 2, 3].map(catches), [true, false, true, false]);
    }

    #[test]
    fn the_light_swells_by_a_quarter() {
        assert_eq!(glow_size(0), 4096);
        assert_eq!(glow_size(16), 4096 + 1024);
        assert_eq!(glow_size(48), 4096 - 1024);
        assert_eq!(glow_size(64), 4096);
    }
}
