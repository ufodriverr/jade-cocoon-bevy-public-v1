
use crate::battle::effects::Kept;
use crate::battle::record::{Random, Record};
use crate::caught_list::Kinds;
use crate::chapters::{Chapter, VariantKey, VariantTable};
use crate::corridor::Corridor;
use crate::creatures::{Caught, Creatures};
use crate::field::Position;
use crate::items::{FREE, Keeping, Kind, SAVED, Worn};
use crate::map;
use crate::name;
use crate::options_screen::Settings;
use crate::pad;
use crate::roamer::ItemCount;
use crate::roaming::Roaming;
use crate::save;
use crate::scene::LocationCommand;

mod at {
    /// What the player has, from the money on.
    pub const KEEPING: usize = 36;
    pub const CHAPTER: usize = 732;
    pub const OWED: usize = 734;
    pub const WORD: usize = 736;
    /// The count of the variant table; its room ends at the player's place.
    pub const VARIANTS: usize = 756;
    pub const PLAYER_PLACE: usize = 1960;
    /// The head of the player's record, and what he has on in it.
    pub const PLAYER: usize = 1992;
    pub const WORN: usize = 2020;
    pub const KINDS: usize = 2068;
    pub const ROAMING: usize = 2104;

    /// What [`super::Beside`] holds: the game's flag word, the play clock, the ten
    /// button masks, the setting word with the window colour behind it, the hero's
    /// name in his record and the word that picks the text of the place.
    pub const FLAGS: usize = 0;
    pub const CLOCK: usize = 4;
    pub const MASKS: usize = 8;
    pub const SETTING: usize = 28;
    pub const COLOUR: usize = 32;
    pub const NAME: usize = 2008;
    pub const PLACE: usize = 2064;

    /// What [`super::Stands`] holds: the wanted area, each wanted value with the
    /// loaded one behind it, and the three angles behind the player's place.
    pub const WANTED: usize = 740;
    pub const ANGLES: usize = 1972;
    /// Which of the player's two models is on, and the value of his weapon.
    pub const MODEL: usize = 2096;
    pub const WEAPON: usize = 2100;
}

/// Bit `0x40` of the game's flag word: the pad is not shaken.
const VIBRATION_OFF: u32 = 0x40;
const FINISHED: u32 = 0x180;
const SECOND_ROUND: u32 = 0x80;
/// The word that picks the text of the place behind the end of the story.
pub const PLACE_OF_A_FINISHED_GAME: i32 = -1;
/// The button masks of the block.
pub const MASKS: usize = 10;
pub const MASKS_AT: u32 = 0x8007_CA18;
pub const MASKS_AT_BOOT: [u16; MASKS] = [
    pad::UP,
    pad::DOWN,
    pad::LEFT,
    pad::RIGHT,
    pad::CIRCLE,
    pad::SQUARE,
    pad::CROSS | pad::L1,
    pad::START,
    pad::TRIANGLE | pad::L2,
    0,
];
pub const PLACE_OF_A_NEW_GAME: i32 = -2;
pub const REFRESHES_A_SECOND: u32 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beside {
    pub clock: u32,
    /// The ten button masks (+8).
    pub masks: [u16; MASKS],
    /// The setting word (+28), the window colour (+32) and bit `0x40` of the flag
    /// word (+0).
    pub settings: Settings,
    /// The hero's name as his record keeps it (+2008), a byte a letter.
    pub name: [u8; name::LETTERS],
    /// The word that picks the text of the place in the head of a save (+2064).
    pub place: i32,
    /// The story is over (the bits `0x180` of the flag word; a save has bit `0x80`): a
    /// load of such a game starts a second round.
    pub finished: bool,
}

impl Beside {
    /// What "Start Game" leaves, with the settings of the session and the name the
    /// name entry gives behind it: the clock at 0, the boot's buttons and the word -2.
    pub fn new_game(settings: Settings, name: [u8; name::LETTERS]) -> Self {
        Self {
            clock: 0,
            masks: MASKS_AT_BOOT,
            settings,
            name,
            place: PLACE_OF_A_NEW_GAME,
            finished: false,
        }
    }

    /// What the field's hand-over does to the game when the script behind the end of
    /// the story asks for the memory card's screen (`0x800B7AE8`): the word of the
    /// place becomes -1 and the bits `0x180` of the flag word are set, whether a save
    /// follows or not.
    pub fn finish(&mut self) {
        (self.place, self.finished) = (PLACE_OF_A_FINISHED_GAME, true);
    }

    /// What the way into a second round does to these: the state setup clears the play
    /// clock and the word of the place, and the new-game task clears bit `0x80` behind
    /// its second movie. (Bit `0x100` stays in the game; nothing asks it, and the build
    /// keeps the two as one.)
    pub fn second_round(&mut self) {
        (self.clock, self.place, self.finished) = (0, 0, false);
    }

    /// Each piece goes into the block where the game has it. Of the flag word only
    /// bit `0x40` and the bits `0x180` are written, and every other byte of the block stays what it is: the
    /// two bytes behind the colour, and the two behind the name's ten letters, which
    /// are zeros in the template of "Start Game". Run it behind
    /// [`Carried::write_block`], which leaves these bytes alone.
    pub fn write_block(&self, block: &mut [u8; save::BLOCK]) {
        let mut flags = u32::from_le_bytes(four(block, at::FLAGS)) & !(VIBRATION_OFF | FINISHED);
        if self.finished {
            flags |= FINISHED;
        }
        if self.settings.vibration_off {
            flags |= VIBRATION_OFF;
        }
        block[at::FLAGS..at::FLAGS + 4].copy_from_slice(&flags.to_le_bytes());
        block[at::CLOCK..at::CLOCK + 4].copy_from_slice(&self.clock.to_le_bytes());
        for (mask, to) in (self.masks.iter()).zip(block[at::MASKS..].chunks_exact_mut(2)) {
            to.copy_from_slice(&mask.to_le_bytes());
        }
        block[at::SETTING..at::SETTING + 4].copy_from_slice(&self.settings.word.to_le_bytes());
        block[at::COLOUR..at::COLOUR + 2].copy_from_slice(&self.settings.colour.to_le_bytes());
        block[at::NAME..at::NAME + name::LETTERS].copy_from_slice(&self.name);
        block[at::PLACE..at::PLACE + 4].copy_from_slice(&self.place.to_le_bytes());
    }

    /// The way back of [`Self::write_block`].
    pub fn read_block(block: &[u8; save::BLOCK]) -> Self {
        let half = |at: usize| u16::from_le_bytes([block[at], block[at + 1]]);
        Self {
            clock: u32::from_le_bytes(four(block, at::CLOCK)),
            masks: std::array::from_fn(|n| half(at::MASKS + 2 * n)),
            settings: Settings {
                word: u32::from_le_bytes(four(block, at::SETTING)),
                colour: half(at::COLOUR),
                vibration_off: u32::from_le_bytes(four(block, at::FLAGS)) & VIBRATION_OFF != 0,
            },
            name: std::array::from_fn(|n| block[at::NAME + n]),
            place: i32::from_le_bytes(four(block, at::PLACE)),
            finished: u32::from_le_bytes(four(block, at::FLAGS)) & SECOND_ROUND != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stands {
    pub area: i16,
    pub place: i16,
    pub room: i16,
    pub view: i16,
    pub position: Position,
    /// The angle about the upright axis, 4096 to the full turn (`0x8008BCB6`).
    pub angle: i16,
}

impl Stands {
    /// Each piece goes into the block where the game has it; the half word at +738
    /// and the two bytes behind the angles stay what they are.
    pub fn write_block(&self, block: &mut [u8; save::BLOCK]) {
        let pairs = [self.area, self.place, self.room, self.view];
        for (n, value) in pairs.into_iter().enumerate() {
            // The wanted one and the loaded one behind it.
            for at in [at::WANTED + 4 * n, at::WANTED + 4 * n + 2] {
                block[at..at + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
        for (n, value) in self.position.into_iter().enumerate() {
            let at = at::PLAYER_PLACE + 4 * n;
            block[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (n, value) in [0, self.angle, 0].into_iter().enumerate() {
            let at = at::ANGLES + 2 * n;
            block[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    /// The way back of [`Self::write_block`], by the wanted values.
    pub fn read_block(block: &[u8; save::BLOCK]) -> Self {
        let half = |at: usize| i16::from_le_bytes([block[at], block[at + 1]]);
        Self {
            area: half(at::WANTED),
            place: half(at::WANTED + 4),
            room: half(at::WANTED + 8),
            view: half(at::WANTED + 12),
            position: std::array::from_fn(|n| {
                i32::from_le_bytes(four(block, at::PLAYER_PLACE + 4 * n))
            }),
            angle: half(at::ANGLES + 2),
        }
    }
}

pub fn write_model(block: &mut [u8; save::BLOCK], worn: Worn, looks: &[usize]) -> bool {
    let Some(look) = looks.get(usize::from(worn[0])) else {
        return false;
    };
    let model = u32::from(worn[1] != FREE);
    block[at::MODEL..at::MODEL + 4].copy_from_slice(&model.to_le_bytes());
    block[at::WEAPON..at::WEAPON + 2].copy_from_slice(&(*look as u16).to_le_bytes());
    true
}

fn four(block: &[u8; save::BLOCK], at: usize) -> [u8; 4] {
    [block[at], block[at + 1], block[at + 2], block[at + 3]]
}

/// The state word +0x2E0 as "Start Game" leaves it.
pub const WORD_OF_A_NEW_GAME: i16 = 1;
/// The story flag that opens the gate's line of a village map.
pub const GATE_FLAG: u32 = 1;
/// The story variable a forest's number is kept in when the place map leaves for one.
pub const FOREST_VARIABLE: u32 = 4;
/// What the player has on as the title overlay's setup of a game leaves it: the first
/// weapon and nothing else.
pub const WORN_AT_THE_START: Worn = [0, FREE, FREE];
/// The equip routine gives back a weapon it takes off when its number is under the first
/// of these, and an armour under the second.
const WEAPONS_GIVEN_BACK: u8 = 23;
const ARMOURS_GIVEN_BACK: u8 = 15;

pub const LEVEL_AT_THE_START: u8 = 1;

/// The state of the running game that a room hands to the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried {
    /// The chapter (+0x2DC).
    pub chapter: usize,
    /// The next chapter whose variant lists are not applied yet (+0x2DE; the -1 of a new
    /// game counts as 0).
    pub owed: usize,
    /// The variant table (+0x2F4).
    pub variants: VariantTable,
    /// The state word +0x2E0: which village the place map shows.
    pub word: i16,
    /// What the player has, the flags of the story (+0x2B4) and the story variables
    /// (+0x2C7), which are the small values behind the flags.
    pub keeping: Keeping,
    /// What the player has on: the weapon, the armour and the third thing, 255 for none
    /// (`0x8008BCE4` to `0x8008BCE6`).
    pub worn: Worn,
    pub player: Option<Record>,
    pub creatures: Creatures,
    /// The twelve records of the creatures he has caught and not yet made his own
    /// (`0x8008CD10`), and the kinds that count as caught (`0x8008BD14`).
    pub caught: Caught,
    pub kinds: Kinds,
    pub roaming: Roaming,
    /// The count of the two items that make the creatures of a room rest or come
    /// (a task of the executable, so it runs on from room to room).
    pub item_count: ItemCount,
    pub random: Random,
    pub effect_half: i16,
    pub effect_kept: Kept,
    pub corridor: Option<Corridor>,
    pub place_word: i32,
}

impl Default for Carried {
    fn default() -> Self {
        Self::new_game()
    }
}

impl Carried {
    /// The state behind "Start Game", before the chapter screen of chapter 0.
    pub fn new_game() -> Self {
        Self {
            chapter: 0,
            owed: 0,
            variants: VariantTable::new(),
            word: WORD_OF_A_NEW_GAME,
            keeping: Keeping::new(),
            worn: WORN_AT_THE_START,
            player: None,
            creatures: Creatures::new_game(),
            caught: Caught::default(),
            kinds: Kinds::default(),
            roaming: Roaming::new_game(),
            item_count: ItemCount::default(),
            random: Random { seed: 0 },
            effect_half: 0,
            effect_kept: Kept::default(),
            corridor: None,
            place_word: PLACE_OF_A_NEW_GAME,
        }
    }

    pub fn second_round(&mut self) {
        (self.chapter, self.owed, self.word) = (0, 0, WORD_OF_A_NEW_GAME);
        self.place_word = 0;
        self.variants = VariantTable::new();
        self.roaming = Roaming::new_game();
    }

    /// The player's level, the byte `0x8008BD0C` (+0x44 of his record): 1 while the
    /// record is still the template of "Start Game".
    pub fn player_level(&self) -> u8 {
        self.player
            .as_ref()
            .map_or(LEVEL_AT_THE_START, |player| player.level)
    }

    pub fn write_block(&self, block: &mut [u8; save::BLOCK], template: &Record) -> bool {
        let Some(variants) = self.variants.to_bytes() else {
            return false;
        };
        let half = |value: usize| (value as i16).to_le_bytes();
        block[at::KEEPING..at::KEEPING + SAVED].copy_from_slice(&self.keeping.to_bytes());
        block[at::CHAPTER..at::CHAPTER + 2].copy_from_slice(&half(self.chapter));
        // A new game has -1 where the build counts 0.
        let owed = if self.owed == 0 {
            [0xFF; 2]
        } else {
            half(self.owed)
        };
        block[at::OWED..at::OWED + 2].copy_from_slice(&owed);
        block[at::WORD..at::WORD + 2].copy_from_slice(&self.word.to_le_bytes());
        block[at::VARIANTS..at::VARIANTS + variants.len()].copy_from_slice(&variants);
        let head: &mut [u8; Record::HEAD] = (&mut block[at::PLAYER..at::PLAYER + Record::HEAD])
            .try_into()
            .expect("the head lies in the block");
        self.player.as_ref().unwrap_or(template).write_head(head);
        block[at::WORN..at::WORN + 3].copy_from_slice(&self.worn);
        for (word, to) in (self.kinds.0.iter()).zip(block[at::KINDS..].chunks_exact_mut(4)) {
            to.copy_from_slice(&word.to_le_bytes());
        }
        block[at::ROAMING..at::ROAMING + Roaming::SAVED]
            .copy_from_slice(&self.roaming.tables_to_bytes());
        // The mark the reset of "Start Game" puts into the block and nothing changes
        // after it: every game that runs has it, and the reader of a save asks for it.
        block[save::MARK_AT..save::MARK_AT + 4].copy_from_slice(&save::MARK.to_le_bytes());
        true
    }

    /// The way back of [`Self::write_block`]: the pieces of a block go over this
    /// state, and what the block does not hold of it stays (the creatures and the
    /// caught ones, which a save has apart, and what is not saved at all). The
    /// player's record is taken with what he has on in its equipment bytes, and counts
    /// as the template's while it is the same. Answers false, and changes nothing,
    /// for a block whose variant table has a count past its room.
    pub fn read_block(&mut self, block: &[u8; save::BLOCK], template: &Record) -> bool {
        let Some(variants) = VariantTable::from_bytes(&block[at::VARIANTS..at::PLAYER_PLACE])
        else {
            return false;
        };
        let half = |at: usize| i16::from_le_bytes([block[at], block[at + 1]]);
        let keeping: &[u8; SAVED] = (&block[at::KEEPING..at::KEEPING + SAVED])
            .try_into()
            .expect("what he has lies in the block");
        self.keeping = Keeping::from_bytes(keeping);
        self.chapter = half(at::CHAPTER).max(0) as usize;
        self.owed = half(at::OWED).max(0) as usize;
        self.word = half(at::WORD);
        self.variants = variants;
        self.worn = [0, 1, 2].map(|n| block[at::WORN + n]);
        self.player = Record::head(&block[at::PLAYER..]).filter(|record| {
            let fresh = Record {
                equipment: record.equipment,
                ..template.clone()
            };
            *record != fresh
        });
        for (word, from) in (self.kinds.0.iter_mut()).zip(block[at::KINDS..].chunks_exact(4)) {
            *word = u32::from_le_bytes([from[0], from[1], from[2], from[3]]);
        }
        let tables: &[u8; Roaming::SAVED] = (&block[at::ROAMING..at::ROAMING + Roaming::SAVED])
            .try_into()
            .expect("the tables lie in the block");
        self.roaming.tables_from_bytes(tables);
        true
    }

    /// The whole of what a save holds of this state: [`Self::write_block`] for the
    /// block, and the creatures and the caught ones over their records. Answers as
    /// `write_block` does.
    pub fn write_memory(&self, memory: &mut save::Memory, template: &Record) -> bool {
        if !self.write_block(&mut memory.block, template) {
            return false;
        }
        self.creatures.write(&mut memory.creatures);
        self.caught.write(&mut memory.caught);
        true
    }

    /// The way back of [`Self::write_memory`].
    pub fn read_memory(&mut self, memory: &save::Memory, template: &Record) -> bool {
        if !self.read_block(&memory.block, template) {
            return false;
        }
        self.creatures = Creatures::read(&memory.creatures);
        self.caught = Caught::read(&memory.caught);
        true
    }

    pub fn make_whole(&mut self) {
        let whole = |record: &mut Record| {
            record.status = 0;
            (record.hp, record.mp) = (record.max_hp, record.max_mp);
        };
        (self.player.iter_mut())
            .chain(self.creatures.carried_mut())
            .for_each(whole);
    }

    pub fn in_front_of_the_list(&self) -> i32 {
        let hurt = |record: &Record| record.hp != record.max_hp;
        let worn = |record: &Record| {
            hurt(record) || record.mp != record.max_mp || record.status & 0x400 != 0
        };
        if self.caught.waiting().next().is_some() {
            1
        } else if self.player.as_ref().is_some_and(hurt) {
            3
        } else if self.creatures.carried().any(worn) {
            2
        } else {
            0
        }
    }

    /// The equip routine (`0x800ADBBC(item, kind)`): kind 0 puts a weapon on, kind 1 an
    /// armour, kind 2 the third thing. A weapon or an armour is not taken off by it: item
    /// 255 changes nothing, and answers `false` while one is on. Behind the store one of
    /// the thing is taken from what the player has, when there is one, and the thing that
    /// was on is given back. Another kind changes nothing and answers `true`.
    pub fn equip(&mut self, item: i32, kind: i32) -> bool {
        let Some(place) = usize::try_from(kind).ok().filter(|place| *place < 3) else {
            return true;
        };
        let was = self.worn[place];
        let (list, given_back) = match place {
            0 => (Kind::Weapon, was < WEAPONS_GIVEN_BACK),
            1 => (Kind::Armour, was < ARMOURS_GIVEN_BACK),
            _ => (Kind::Ornament, was != FREE),
        };
        if place < 2 && item == i32::from(FREE) {
            return was == FREE;
        }
        self.worn[place] = item as u8;
        self.keeping.take(item, 1, list);
        if given_back {
            self.keeping.give(i32::from(was), 1, list, self.worn);
        }
        true
    }

    /// What the chapter screen does to the state when it leaves for the room of chapter
    /// `number`: the story variables of its list are cleared, the variant lists of every
    /// chapter from the next one to apply up to this one are applied, and the state word
    /// gets the value of the chapter's record. `chapters` are the records from chapter 0
    /// on. Answers false, and changes nothing, when there is no such chapter.
    pub fn start_chapter<'a>(
        &mut self,
        number: usize,
        chapters: impl IntoIterator<Item = &'a Chapter>,
        cleared: &[i32],
    ) -> bool {
        let chapters: Vec<&Chapter> = chapters.into_iter().take(number + 1).collect();
        let Some(chapter) = chapters.get(number) else {
            return false;
        };
        for &variable in cleared {
            if let Ok(variable) = u32::try_from(variable) {
                self.set_variable(variable, 0);
            }
        }
        for chapter in chapters.iter().skip(self.owed) {
            self.variants.apply(chapter);
        }
        self.chapter = number;
        self.owed = number + 1;
        self.word = chapter.state_word as i16;
        true
    }

    /// A stand-in for the story so far: a new game whose chapter screens have run up to
    /// chapter `number`, with nothing done in the rooms between them.
    pub fn at_chapter<'a>(
        number: usize,
        chapters: impl IntoIterator<Item = &'a Chapter>,
    ) -> Option<Self> {
        let mut carried = Self::new_game();
        carried
            .start_chapter(number, chapters, &[])
            .then_some(carried)
    }

    /// Runs a location command that only writes to the state: `0x50` and `0x51`. Answers
    /// whether the command was one of the two.
    pub fn run(&mut self, command: LocationCommand) -> bool {
        match command {
            LocationCommand::SetStateWord { value } => self.word = value as i16,
            LocationCommand::SetVariant {
                area,
                place,
                slot,
                room,
            } => {
                let key = VariantKey {
                    area: area as i16,
                    place: place as i16,
                    slot: slot as i16,
                };
                self.variants.set(key, room as i16);
            }
            _ => return false,
        }
        true
    }

    /// A story variable (`0x800AD324`): a byte.
    pub fn variable(&self, number: u32) -> i32 {
        i32::from(self.keeping.byte(number as usize))
    }

    /// Writes a story variable (`0x800AD33C`): the low byte of the value is kept.
    pub fn set_variable(&mut self, number: u32, value: i32) {
        self.keeping.set_byte(number as usize, value as u8);
    }

    /// What the place map asks of the state.
    pub fn map_state(&self) -> map::State<'_> {
        map::State {
            variants: &self.variants,
            word: self.word,
            gate_flag: self.keeping.flag(GATE_FLAG),
        }
    }

    /// What choosing an entry of the place map writes besides the wanted location: a
    /// forest's number.
    pub fn choose(&mut self, choice: &map::Choice) {
        if let Some(forest) = choice.forest {
            self.set_variable(FOREST_VARIABLE, i32::from(forest));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keeper_of_the_list_sees_what_there_is_to_do() {
        let record = |hp: i16, mp: i16, status: u32| {
            let mut record = Record::head(&[0u8; Record::HEAD]).expect("a head");
            (record.max_hp, record.max_mp) = (10, 5);
            (record.hp, record.mp, record.status) = (hp, mp, status);
            record
        };
        let carrying = crate::creatures::state::CARRIED;
        // A new game: nothing waits and nobody is hurt.
        let mut carried = Carried::new_game();
        assert_eq!(carried.in_front_of_the_list(), 0);
        // A carried creature without all its mana, or with the status bit.
        carried.creatures.put(carrying, record(10, 4, 0));
        assert_eq!(carried.in_front_of_the_list(), 2);
        let mut carried = Carried::new_game();
        carried.creatures.put(carrying, record(10, 5, 0x400));
        assert_eq!(carried.in_front_of_the_list(), 2);
        // The player's own HP come before the creatures', a caught creature before both.
        carried.player = Some(record(9, 5, 0));
        assert_eq!(carried.in_front_of_the_list(), 3);
        carried.caught.put(record(10, 5, 0));
        assert_eq!(carried.in_front_of_the_list(), 1);
    }

    fn key(area: i16, place: i16, slot: i16) -> VariantKey {
        VariantKey { area, place, slot }
    }

    /// A stand-in for the template of "Start Game": HP 100 of 100 and level 1.
    fn template() -> Record {
        let mut record = Record::head(&[0u8; Record::HEAD]).expect("a head");
        (record.hp, record.max_hp, record.level, record.fixed) = (100, 100, 1, true);
        record
    }

    #[test]
    fn a_new_game_lies_in_the_block_as_the_reset_leaves_it() {
        let mut block = [0xEE; save::BLOCK];
        assert!(Carried::new_game().write_block(&mut block, &template()));
        let half = |at: usize| i16::from_le_bytes([block[at], block[at + 1]]);
        // No money, every pair free, no cocoons, no flag.
        assert_eq!(block[36..40], [0; 4]);
        assert!(block[40..680].chunks(2).all(|pair| pair == [FREE, 0]));
        assert_eq!(block[680..731], [0; 51]);
        // Chapter 0, nothing applied yet, the state word 1, an empty variant table.
        assert_eq!((half(732), half(734), half(736)), (0, -1, 1));
        assert_eq!(block[756..760], [0; 4]);
        // The template with weapon 0 and nothing else on.
        assert_eq!((half(2000), half(2002)), (100, 100));
        assert_eq!(block[2020..2023], [0, FREE, FREE]);
        assert_eq!((block[2060], block[2061]), (1, 1));
        // No kind caught, every room of the forests to be filled anew.
        assert_eq!(block[2068..2096], [0; 28]);
        assert!(block[2104..2432].chunks(2).all(|word| word == [0, 0x80]));
        // What the build does not keep is not touched: the clock and the settings, the
        // gap behind the variables, where he is, his name, the word of the place, the
        // values of his model. The mark of a save is written.
        assert_eq!(block[2436..2440], [5, 0, 0, 0]);
        let untouched = [
            0..36,
            731..732,
            738..756,
            760..1992,
            2008..2020,
            2023..2024,
            2064..2068,
            2096..2104,
            2432..2436,
        ];
        for bytes in untouched {
            assert!(block[bytes.clone()].iter().all(|b| *b == 0xEE), "{bytes:?}");
        }
    }

    #[test]
    fn the_block_gives_back_what_was_put_into_it() {
        let mut carried = Carried::at_chapter(1, &chapters()).expect("chapter 1");
        carried.variants.set(key(4, 1, 3), 106);
        assert!(carried.keeping.give_money(1234));
        assert!(carried.keeping.give(5, 2, Kind::Weapon, carried.worn));
        assert!(carried.keeping.give_cocoons(3));
        assert!(carried.keeping.give_story_thing(33));
        carried.keeping.set_flag(GATE_FLAG);
        carried.set_variable(FOREST_VARIABLE, 2);
        carried.worn = [5, 3, FREE];
        let mut player = template();
        (player.hp, player.experience, player.status) = (37, 0x0812_3456, 0x400);
        (player.modifiers[11], player.stats[4], player.equipment) = (-32, 77, carried.worn);
        carried.player = Some(player);
        carried.kinds.mark(4);
        carried.kinds.mark(70);
        let mut tables = carried.roaming.tables_to_bytes();
        tables[4..6].copy_from_slice(&[0x15, 0]);
        carried.roaming.tables_from_bytes(&tables);
        assert_eq!(carried.roaming.word(4, 1, 2), Some(0x0015));

        let mut block = [0; save::BLOCK];
        assert!(carried.write_block(&mut block, &template()));
        assert_eq!(block[36..40], 1234i32.to_le_bytes());
        assert_eq!(block[692] & 2, 2);
        assert_eq!(block[711 + FOREST_VARIABLE as usize], 2);
        assert_eq!(block[2104 + 4..2104 + 6], [0x15, 0]);
        let mut loaded = Carried::new_game();
        loaded.roaming.plain = false;
        assert!(loaded.read_block(&block, &template()));
        carried.roaming.plain = false;
        assert_eq!(loaded, carried);

        // A record that is still the template's is none of his own.
        carried.player = None;
        assert!(carried.write_block(&mut block, &template()));
        assert!(loaded.read_block(&block, &template()));
        assert_eq!(loaded.player, None);
        assert_eq!(loaded, carried);
    }

    /// A creature with something in every field a record keeps.
    fn creature(species: u32) -> Record {
        let mut record = template();
        (record.species, record.spells, record.fixed) = (species, 0x12, false);
        (record.hp, record.mp, record.max_mp, record.level) = (41, 7, 9, 6);
        (record.experience, record.element_bit) = (0x0123_4567, 2);
        (record.equipment, record.strengths) = ([255; 3], [0, 32, 0, 0]);
        record.stats = [30, 31, 32, 33, 34, 35];
        record.growth = [150, 140, 40, 50, 120, 100];
        record.progress = [1, 2, 3, 4, 5, 6];
        record.modifiers = [32, 64, 32, 16, 32, 32, 32, 32, 32, -32, 32, 32];
        record.name = [9, 8, 7, 6, 5, 4, 3, 2, 1, 0];
        record.moves[1] = crate::battle::Move {
            known: true,
            attack: 12,
            grade: 3,
            flag: 1,
        };
        record
    }

    #[test]
    fn a_whole_state_goes_through_a_save_and_comes_back() {
        use crate::creatures::state::{CARRIED, KEPT};
        let mut carried = Carried::at_chapter(1, &chapters()).expect("chapter 1");
        assert!(carried.keeping.give_money(77));
        carried.creatures.put(CARRIED, creature(4));
        carried.creatures.put(KEPT, creature(60));
        carried.caught.put(creature(21));
        carried.kinds.mark(21);

        // Into the game's memory and back, byte for byte the same state.
        let mut memory = save::Memory::zeroed();
        assert!(carried.write_memory(&mut memory, &template()));
        assert_eq!(memory.creatures[0][0..4], [3, 0, 4, 4]);
        assert_eq!(memory.creatures[2][0..2], [4, 0]);
        assert_eq!(memory.caught[1][0x5C..0x60], 211u32.to_le_bytes());
        let mut loaded = Carried::new_game();
        assert!(loaded.read_memory(&memory, &template()));
        assert_eq!(loaded, carried);

        // Through the buffer of a save into a memory that holds nothing: the block has
        // the mark the reader asks for, and the creatures come back whole.
        let mut buffer = vec![0; save::END];
        assert!(memory.write(&mut buffer, &[0; 24], &[[0; 24]; 2]));
        let mut fresh = save::Memory::zeroed();
        assert!(fresh.read(&buffer, false));
        let mut loaded = Carried::new_game();
        assert!(loaded.read_memory(&fresh, &template()));
        assert_eq!(loaded.creatures, carried.creatures);
        assert_eq!((loaded.keeping.money(), loaded.chapter), (77, 1));
        let was = carried.caught.waiting().next().expect("the caught one");
        let back = loaded.caught.waiting().next().expect("the caught one");
        assert_eq!((back.spells, back.moves), (0, Default::default()));
        let whole = Record {
            spells: was.spells,
            moves: was.moves,
            ..back.clone()
        };
        assert_eq!(&whole, was);
    }

    #[test]
    fn what_the_build_keeps_beside_lies_in_the_block_and_comes_back() {
        let beside = Beside {
            clock: 0x0102_0304,
            masks: std::array::from_fn(|n| 0x1100 + n as u16),
            settings: Settings {
                word: 0xA1B2_C3D4,
                colour: 3,
                vibration_off: true,
            },
            name: [21, 22, 23, 24, 25, 0, 0, 0, 0, 0],
            place: -1,
            finished: true,
        };
        let mut block = [0xEE; save::BLOCK];
        beside.write_block(&mut block);
        // The flag word keeps its other bits; bit 0x40 is the vibration's, and the
        // bits 0x180 are those of a finished game.
        assert_eq!(block[0..4], [0xEE, 0xEF, 0xEE, 0xEE]);
        assert_eq!(block[4..8], [4, 3, 2, 1]);
        assert_eq!(block[8..12], [0x00, 0x11, 0x01, 0x11]);
        assert_eq!(block[26..28], [0x09, 0x11]);
        assert_eq!(block[28..34], [0xD4, 0xC3, 0xB2, 0xA1, 3, 0]);
        assert_eq!(block[2008..2014], [21, 22, 23, 24, 25, 0]);
        assert_eq!(block[2064..2068], [0xFF; 4]);
        // Nothing else is touched, and nothing of it is what `Carried` writes.
        for bytes in [34..2008, 2018..2064, 2068..save::BLOCK] {
            assert!(block[bytes.clone()].iter().all(|b| *b == 0xEE), "{bytes:?}");
        }
        assert_eq!(Beside::read_block(&block), beside);

        // The vibration on clears its bit and leaves the others.
        let on = Beside {
            settings: Settings {
                vibration_off: false,
                ..beside.settings
            },
            ..beside
        };
        on.write_block(&mut block);
        assert_eq!(block[0..4], [0xAE, 0xEF, 0xEE, 0xEE]);
        assert_eq!(Beside::read_block(&block), on);

        // A game that is not finished has neither of the two bits.
        let running = Beside {
            finished: false,
            ..on
        };
        running.write_block(&mut block);
        assert_eq!(block[0..4], [0x2E, 0xEE, 0xEE, 0xEE]);
        assert_eq!(Beside::read_block(&block), running);
        let mut finished = running;
        finished.finish();
        assert_eq!((finished.place, finished.finished), (-1, true));

        // The way into a second round clears the clock, the word and the mark, and the
        // state setup leaves what the hero has and is.
        finished.second_round();
        assert_eq!(
            (finished.clock, finished.place, finished.finished),
            (0, 0, false)
        );
        assert_eq!(finished.name, running.name);
        let mut carried = Carried::new_game();
        (carried.chapter, carried.owed, carried.word) = (6, 6, 2);
        carried.roaming.plain = false;
        let ended = carried.clone();
        carried.second_round();
        assert_eq!((carried.chapter, carried.owed, carried.word), (0, 0, 1));
        assert_eq!((ended.place_word, carried.place_word), (-2, 0));
        assert_eq!(carried.roaming, Roaming::new_game());
        assert_eq!(
            (&carried.keeping, &carried.creatures, carried.worn),
            (&ended.keeping, &ended.creatures, ended.worn)
        );

        // Behind the state of a new game the two make the block a save is taken from:
        // the head of a save names the hero and the clock, and the reader takes it.
        let named = Beside {
            clock: 3 * 216_000 + 25 * 3600 + 59,
            ..Beside::new_game(Settings::at_boot(), beside.name)
        };
        let mut memory = save::Memory::zeroed();
        assert!(Carried::new_game().write_memory(&mut memory, &template()));
        named.write_block(&mut memory.block);
        let mut buffer = vec![0; save::END];
        assert!(memory.write(&mut buffer, &[7; 24], &[[0; 24]; 2]));
        let head = save::Head::read(&buffer).expect("a head");
        assert_eq!(head.name[..10], beside.name);
        assert_eq!((head.hours, head.minutes), (3, 25));
        assert_eq!((head.place_kind, head.place), (0, [7; 24]));
        let mut fresh = save::Memory::zeroed();
        assert!(fresh.read(&buffer, false));
        assert_eq!(Beside::read_block(&fresh.block), named);
        assert_eq!(named.masks, MASKS_AT_BOOT);
    }

    #[test]
    fn where_he_stands_and_his_model_lie_in_the_block() {
        let stands = Stands {
            area: 2,
            place: 1,
            room: 3,
            view: 4,
            position: [1122, -690, 1906],
            angle: -1024,
        };
        let mut block = [0xEE; save::BLOCK];
        stands.write_block(&mut block);
        // Each wanted value with the loaded one behind it, the place and the angles.
        assert_eq!(block[740..748], [2, 0, 2, 0, 1, 0, 1, 0]);
        assert_eq!(block[748..756], [3, 0, 3, 0, 4, 0, 4, 0]);
        assert_eq!(block[1960..1964], 1122i32.to_le_bytes());
        assert_eq!(block[1964..1968], (-690i32).to_le_bytes());
        assert_eq!(block[1972..1978], [0, 0, 0, 0xFC, 0, 0]);
        for bytes in [0..740, 756..1960, 1978..save::BLOCK] {
            assert!(block[bytes.clone()].iter().all(|b| *b == 0xEE), "{bytes:?}");
        }
        assert_eq!(Stands::read_block(&block), stands);

        // The model: weapon 2 has the value 7 here; with an armour the second model.
        let looks = [0, 0, 7];
        let mut block = [0xEE; save::BLOCK];
        assert!(write_model(&mut block, [2, FREE, FREE], &looks));
        assert_eq!(block[2096..2104], [0, 0, 0, 0, 7, 0, 0xEE, 0xEE]);
        assert!(write_model(&mut block, [1, 4, FREE], &looks));
        assert_eq!(block[2096..2102], [1, 0, 0, 0, 0, 0]);
        // A weapon without a record writes nothing.
        let before = block;
        assert!(!write_model(&mut block, [3, 4, FREE], &looks));
        assert_eq!(block, before);
        assert!(block[..2096].iter().all(|b| *b == 0xEE));
    }

    #[test]
    fn a_variant_table_past_its_room_is_refused_both_ways() {
        let mut carried = Carried::new_game();
        for slot in 0..=VariantTable::ROOM as i16 {
            carried.variants.set(key(1, 1, slot), slot);
        }
        let mut block = [0; save::BLOCK];
        assert!(!carried.write_block(&mut block, &template()));
        assert_eq!(block, [0; save::BLOCK]);
        // The table at its room fills the block to the player's place.
        carried.variants = VariantTable::new();
        for slot in 0..VariantTable::ROOM as i16 {
            carried.variants.set(key(1, 1, slot), slot);
        }
        assert!(carried.write_block(&mut block, &template()));
        assert_eq!(block[1952..1960], [1, 0, 1, 0, 149, 0, 149, 0]);
        let mut loaded = Carried::new_game();
        assert!(loaded.read_block(&block, &template()));
        assert_eq!(loaded.variants, carried.variants);
        // A count past the room.
        block[756..760].copy_from_slice(&151u32.to_le_bytes());
        let before = loaded.clone();
        assert!(!loaded.read_block(&block, &template()));
        assert_eq!(loaded, before);
    }

    fn chapter(state_word: i32, places: &[(VariantKey, i16)]) -> Chapter {
        Chapter {
            state_word,
            start_room: 0,
            music: 0,
            ends_music: false,
            movie: 0,
            place_variants: places.to_vec(),
            extra_variants: Vec::new(),
        }
    }

    fn chapters() -> Vec<Chapter> {
        vec![
            chapter(1, &[(key(2, 0, 0), 67), (key(2, 5, 0), 84)]),
            chapter(0, &[(key(1, 0, 0), 3), (key(2, 0, 0), -1)]),
        ]
    }

    #[test]
    fn a_new_game_has_nothing_but_the_state_word() {
        let carried = Carried::new_game();
        assert_eq!((carried.chapter, carried.owed, carried.word), (0, 0, 1));
        assert!(carried.variants.entries().is_empty());
        assert_eq!(carried.keeping, Keeping::new());
        assert_eq!(carried.worn, [0, FREE, FREE]);
        assert_eq!(carried, Carried::default());
        assert_eq!(carried.player, None);
        assert_eq!(carried.roaming, Roaming::new_game());
    }

    #[test]
    fn the_command_that_makes_whole_clears_the_status_and_fills_hp_and_mp() {
        let mut carried = Carried::new_game();
        // Without a record of his own the player is the template, which is whole.
        carried.make_whole();
        assert_eq!(carried.player, None);
        let mut player = crate::battle::record::tests::head(&"00".repeat(72));
        (player.status, player.hp, player.max_hp) = (0x0403, 1, 100);
        (player.mp, player.max_mp) = (2, 30);
        // A creature he carries is made whole with him, one he only keeps is not.
        let mut hurt = player.clone();
        (hurt.hp, hurt.max_hp, hurt.mp, hurt.max_mp) = (3, 40, 0, 9);
        use crate::creatures::state::{CARRIED, KEPT};
        carried.creatures.put(CARRIED, hurt.clone());
        carried.creatures.put(KEPT, hurt);
        carried.player = Some(player);
        carried.make_whole();
        let player = carried.player.as_ref().expect("his record");
        assert_eq!((player.status, player.hp, player.mp), (0, 100, 30));
        let of = |state| {
            let creature = carried.creatures.nth(state, 0).expect("the creature");
            let record = &creature.record;
            (record.status, record.hp, record.mp)
        };
        assert_eq!(of(CARRIED), (0, 40, 9));
        assert_eq!(of(KEPT), (0x0403, 3, 0));
    }

    #[test]
    fn the_equip_routine_changes_what_is_on_and_trades_with_what_the_player_has() {
        let mut carried = Carried::new_game();
        // An armour the player does not have is put on all the same, and nothing comes
        // back for the place that was empty.
        assert!(carried.equip(0, 1));
        assert_eq!(carried.worn, [0, 0, FREE]);
        assert_eq!(carried.keeping, Keeping::new());
        // Neither a weapon nor an armour is taken off.
        let before = carried.clone();
        assert!(!carried.equip(255, 0));
        assert!(!carried.equip(255, 1));
        assert_eq!(carried, before);
        // An empty place stays empty, and that is no failure.
        let mut bare = Carried::new_game();
        assert!(bare.equip(255, 1));
        assert_eq!(bare, Carried::new_game());

        // Another weapon: one of it leaves the list, the one that was on comes back.
        assert!(carried.keeping.give(5, 2, Kind::Weapon, carried.worn));
        assert!(carried.equip(5, 0));
        assert_eq!(carried.worn, [5, 0, FREE]);
        assert_eq!(carried.keeping.count(5, Kind::Weapon), 1);
        assert_eq!(carried.keeping.count(0, Kind::Weapon), 1);
        // An armour past the fifteenth is not given back.
        assert!(carried.equip(15, 1));
        assert!(carried.equip(3, 1));
        assert_eq!(carried.worn, [5, 3, FREE]);
        assert_eq!(carried.keeping.count(0, Kind::Armour), 1);
        assert_eq!(carried.keeping.count(15, Kind::Armour), 0);

        // The third thing is put on and taken off freely.
        assert!(carried.equip(7, 2));
        assert!(carried.equip(255, 2));
        assert_eq!(carried.worn, [5, 3, FREE]);
        assert_eq!(carried.keeping.count(7, Kind::Ornament), 1);
        // No other kind is anything to the routine.
        let before = carried.clone();
        assert!(carried.equip(1, 3));
        assert!(carried.equip(1, -1));
        assert_eq!(carried, before);
    }

    #[test]
    fn the_chapter_screen_applies_the_lists_that_are_owed() {
        let chapters = chapters();
        let mut carried = Carried::new_game();
        carried.set_variable(7, 3);
        carried.set_variable(6, 5);
        assert!(carried.start_chapter(0, &chapters, &[7, 8, 9]));
        assert_eq!((carried.chapter, carried.owed, carried.word), (0, 1, 1));
        assert_eq!(carried.variants.get(key(2, 0, 0)), Some(67));
        // The variables of the list are cleared, the others stay.
        assert_eq!((carried.variable(7), carried.variable(6)), (0, 5));

        // What a room wrote in between stays when the next chapter does not list it, and
        // the lists of chapter 0 are not applied a second time.
        carried.variants.set(key(2, 5, 0), 85);
        assert!(carried.start_chapter(1, &chapters, &[]));
        assert_eq!((carried.chapter, carried.owed, carried.word), (1, 2, 0));
        assert_eq!(carried.variants.get(key(2, 5, 0)), Some(85));
        assert_eq!(carried.variants.get(key(1, 0, 0)), Some(3));
        assert_eq!(carried.variants.get(key(2, 0, 0)), None);

        // A chapter the table does not have changes nothing.
        let before = carried.clone();
        assert!(!carried.start_chapter(2, &chapters, &[6]));
        assert_eq!(carried, before);
    }

    #[test]
    fn the_story_so_far_is_every_chapter_screen_up_to_the_chapter() {
        let chapters = chapters();
        let carried = Carried::at_chapter(1, &chapters).expect("chapter 1");
        assert_eq!((carried.chapter, carried.owed, carried.word), (1, 2, 0));
        assert_eq!(carried.variants.get(key(2, 5, 0)), Some(84));
        assert_eq!(carried.variants.get(key(2, 0, 0)), None);
        assert_eq!(Carried::at_chapter(2, &chapters), None);
    }

    #[test]
    fn the_writing_location_commands_change_the_state() {
        let mut carried = Carried::at_chapter(0, &chapters()).expect("chapter 0");
        assert!(carried.run(LocationCommand::SetStateWord { value: 0 }));
        assert_eq!(carried.word, 0);
        let leads = LocationCommand::SetVariant {
            area: 2,
            place: 5,
            slot: 3,
            room: 86,
        };
        assert!(carried.run(leads));
        assert_eq!(carried.variants.get(key(2, 5, 3)), Some(86));
        // -1 closes an entry.
        let closes = LocationCommand::SetVariant {
            area: 2,
            place: 0,
            slot: 0,
            room: -1,
        };
        assert!(carried.run(closes));
        assert!(!carried.variants.is_open(2, 0));
        // The commands that send the game somewhere are not run here.
        let before = carried.clone();
        assert!(!carried.run(LocationCommand::GoToRoom { room: 2 }));
        assert!(!carried.run(LocationCommand::OpenPlaceMap));
        assert_eq!(carried, before);
    }

    #[test]
    fn the_story_variables_are_the_small_values_behind_the_flags() {
        let mut carried = Carried::new_game();
        carried.set_variable(0, 1);
        carried.set_variable(11, 0x1FF);
        assert_eq!(carried.keeping.byte(0), 1);
        // A variable is a byte.
        assert_eq!(carried.variable(11), 0xFF);
        assert_eq!(carried.variable(12), 0);
        // The first variable lies right behind the last flag.
        assert!(carried.keeping.flag(crate::items::FLAGS));
    }

    #[test]
    fn the_place_map_reads_the_state() {
        let mut carried = Carried::at_chapter(0, &chapters()).expect("chapter 0");
        let state = carried.map_state();
        assert_eq!((state.word, state.gate_flag), (1, false));
        assert!(state.variants.is_open(2, 5));
        carried.keeping.set_flag(GATE_FLAG);
        carried.run(LocationCommand::SetStateWord { value: 0 });
        let state = carried.map_state();
        assert_eq!((state.word, state.gate_flag), (0, true));

        // A forest's number is kept, a place of the village leaves the variable alone.
        let forest = map::Choice {
            area: 4,
            place: 0,
            forest: Some(2),
        };
        carried.choose(&forest);
        assert_eq!(carried.variable(FOREST_VARIABLE), 2);
        let place = map::Choice {
            area: 2,
            place: 5,
            forest: None,
        };
        carried.choose(&place);
        assert_eq!(carried.variable(FOREST_VARIABLE), 2);
    }
}
