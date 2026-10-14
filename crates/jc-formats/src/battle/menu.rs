
use super::fight::{Choice, Fight, OPPONENTS, PLAYER, SLOTS};
use super::record::{Draws, Record, Side};
use super::resolve::{Command, Order, drains, resolve};
use super::{Tables, item, using};
use crate::menu::{self, Cursor, cursor};
use crate::pad::{self, Pad};
use crate::player_panel::{AttackRow, SpellRows, attacks, spell_at, spells, spells_known};

/// The levels of the menu.
pub mod level {
    pub const ROOT: u8 = 0;
    pub const ITEMS: u8 = 1;
    pub const ATTACKS: u8 = 2;
    pub const SPELLS: u8 = 3;
    pub const TARGET: u8 = 4;
    pub const DONE: u8 = 5;
}

/// The numbers of the root commands.
pub mod command {
    pub const ATTACK: u8 = 0;
    pub const DEFEND: u8 = 1;
    pub const SUMMON: u8 = 2;
    pub const CAPTURE: u8 = 3;
    pub const ITEM: u8 = 4;
    pub const FLEE: u8 = 5;
    pub const BODY: u8 = 6;
    pub const SPELL: u8 = 7;
    pub const CHANGE: u8 = 8;
}

/// The bits of a target class.
pub mod class {
    /// Everybody of the side at once, without picking.
    pub const EVERYBODY: u8 = 1;
    pub const OWN: u8 = 2;
    pub const OPPONENTS: u8 = 4;
}

/// The sounds of the menu.
pub mod sound {
    pub const REFUSED: u32 = 124;
    pub const CANCELLED: u32 = 125;
    pub const CONFIRMED: u32 = 126;
    pub const MOVED: u32 = 127;
}

/// The messages of a refusal: places in the menu's table of texts (`0x800C9520`).
pub mod text {
    pub const NO_CREATURE: u8 = 10;
    pub const CANNOT_COME: u8 = 11;
    pub const NO_MANA: u8 = 12;
    pub const CANNOT_CAPTURE: u8 = 13;
    pub const NO_COCOON: u8 = 14;
    pub const CANNOT_FLEE: u8 = 15;
    pub const NO_NEED: u8 = 16;
    pub const FOR_CREATURES: u8 = 18;
    pub const NOT_FOR_CREATURES: u8 = 19;
    pub const NOT_HERE: u8 = 20;
}

/// The bits of [`Menu::draws`]: what a pass leaves out of its picture.
pub mod leaves_out {
    pub const ROOT: u16 = 0x04;
    pub const PICKER: u16 = 0x10;
    pub const ITEMS: u16 = 0x20;
    pub const ATTACKS: u16 = 0x40;
    pub const SPELLS: u16 = 0x80;
}

pub mod sets {
    pub const OPPONENTS_AWAY: u16 = 0x01;
    pub const OWN_AWAY: u16 = 0x02;
}

/// The command of a root cell by the side and the cursor, and by the command the level
/// behind it, its target class and its kind.
const COMMANDS: [[u8; 6]; 2] = [[0, 1, 2, 3, 4, 5], [0, 1, 6, 7, 8, 5]];
const NEXT: [u8; 9] = [4, 5, 4, 4, 1, 5, 2, 3, 4];
const CLASS: [u8; 9] = [4, 0, 2, 4, 0, 0, 4, 4, 2];
const KIND: [u32; 9] = [0, 18, 2, 5, 4, 7, 6, 1, 8];
/// What a pass draws, by the level and by whether a text is up.
const DRAWS: [[u16; 2]; 6] = [
    [0x1F0, 0x0F0],
    [0x1D1, 0x1D7],
    [0x1B1, 0x1B7],
    [0x170, 0x176],
    [0x1E0, 0x0F6],
    [0, 0],
];
/// The test of a spell's target (`0x800340C4`): the bit of a spell's word that says it
/// is for the own side, the status bits a curing spell is needed for, and the curing
/// spell that is always taken.
const FOR_THE_OWN_SIDE: u32 = 0x40;
const CURABLE: u32 = 0x7FB;
const CURES_ALWAYS: u32 = 16;
/// The body attack that goes to every opponent at once (`0x80031B68`).
const ALL_AT_ONCE: u8 = 12;
/// The passes a message stays without a button.
const MESSAGE: u8 = 150;
/// The passes the root takes to open.
const OPEN: u8 = 8;

/// What the maker of a menu notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// The slot that chooses: 0 the player, 1 to 3 his creature.
    pub fighter: usize,
    /// The masks of the carried creatures and of the opponents on the field (1 << slot).
    pub carried: u8,
    pub opponents: u8,
    /// An opponent on the field cannot be caught, no cocoon is empty, the fight has no
    /// retreat: what closes "capture" and "flee". "Summon" is closed without a carried
    /// creature.
    pub boss: bool,
    pub no_cocoon: bool,
    pub no_retreat: bool,
    /// The last place in use of the items plus one, and how many spells the creature
    /// knows.
    pub places: u16,
    pub spells: u16,
    /// The cursor among the opponents as the last menu left it.
    pub kept: i16,
}

/// A row of a list that passed its test: what it makes of the command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listed {
    pub parameter: u32,
    pub class: u8,
}

/// A refused confirm: the message, or none.
pub type Refused = Option<u8>;

/// What the menu asks of the lists and of the executable's tests.
pub trait Asked {
    /// The item at a place of the list (`0x80033B58`), the body attack and the spell of
    /// a row (`0x80033CF4`, `0x80033C64`).
    fn item(&self, place: usize) -> Result<Listed, Refused>;
    fn attack(&self, row: usize) -> Result<Listed, Refused>;
    fn spell(&self, row: usize) -> Result<Listed, Refused>;
    /// The slot the picker of the own side answers for a row (`0x8002D2AC`); the row is
    /// -1 for a class that picks nobody.
    fn own(&self, command: u8, parameter: u32, row: i16) -> usize;
    /// The test of a target for an item, a body attack, a spell, a summon and a change
    /// (`0x80033E50`, `0x80034180`, `0x800340C4`, the dead creature).
    fn target(&self, command: u8, parameter: u32, picked: usize) -> Result<(), Refused>;
    /// Whether the circle has a description for a line of a list: an item is at the
    /// place, the spell has a text.
    fn described(&self, level: u8, line: usize) -> bool;
}

pub struct Lists<'a> {
    pub fight: &'a Fight,
    pub tables: &'a Tables,
    /// The item at each place of the player's list of items; `None` for a free place.
    pub places: &'a [Option<usize>],
}

impl Lists<'_> {
    /// The carried creatures by slot (`0x800C0C6C`: a bit for each of the slots 1 to as
    /// many as the records in state 3, which are the slots of a fight that have one).
    fn carried(&self) -> impl Iterator<Item = usize> + '_ {
        (1..OPPONENTS).filter(|slot| self.fight.records[*slot].is_some())
    }

    /// What the maker of the menu notes for the fighter that chooses; `kept` is the
    /// cursor among the opponents as the last menu left it.
    pub fn offer(&self, kept: i16) -> Offer {
        let fight = self.fight;
        let last = self.last_held();
        Offer {
            fighter: fight.fighter,
            carried: self.carried().fold(0, |mask, slot| mask | 1 << slot),
            opponents: fight.opponents(),
            boss: fight.boss_stands(),
            no_cocoon: fight.cocoons == 0,
            no_retreat: !fight.retreat,
            places: last.map_or(0, |last| last as u16 + 1),
            spells: self.chooser().map_or(0, |record| {
                crate::player_panel::spells_known(record.spells) as u16
            }),
            kept,
        }
    }

    /// The record of the creature that chooses; none while the player does.
    fn chooser(&self) -> Option<&Record> {
        let fighter = self.fight.fighter;
        (self.fight.records.get(fighter)?.as_ref()).filter(|_| fighter != PLAYER)
    }

    /// The item at a place of the list and the pieces left of it. A use takes its piece
    /// from the player's own list (`0x800AD3B0`), which frees the pair with the last
    /// one: a place whose item the fight has no piece of is free.
    fn held(&self, place: usize) -> Option<(usize, u32)> {
        let number = (*self.places.get(place)?)?;
        let pieces = self.fight.items.iter().find(|entry| entry.0 == number)?.1;
        (pieces != 0).then_some((number, pieces))
    }

    /// The last place of the list that is not free.
    fn last_held(&self) -> Option<usize> {
        (0..self.places.len()).rposition(|place| self.held(place).is_some())
    }

    /// The item of a place with its flags and the pieces left of it.
    fn at(&self, place: usize) -> Option<(usize, u16, u32)> {
        let (number, pieces) = self.held(place)?;
        let flags = self.tables.items.get(number)?.flags;
        Some((number, flags, pieces))
    }
}

impl Lists<'_> {
    /// A place of the list as a row shows it: the item, how many pieces are left of it,
    /// and whether the test of the row refuses it. A free place has no piece.
    pub fn row(&self, place: usize) -> crate::player_panel::ItemRow {
        let Some((number, _, pieces)) = self.at(place) else {
            return Default::default();
        };
        crate::player_panel::ItemRow {
            item: number.min(255) as u8,
            count: pieces.min(255) as u8,
            refused: self.item(place).is_err(),
        }
    }

    /// The rows of the picker of a target on the own side for the menu's command, each
    /// with its slot and whether it is shown closed: a row is open exactly where the
    /// confirm would be taken ("Items in a fight").
    pub fn picker(&self, menu: &Menu) -> Vec<(usize, bool)> {
        let (command, parameter) = (menu.command(), menu.parameter());
        (0..menu.own().rows)
            .map(|row| {
                let slot = self.own(command, parameter, row);
                (slot, self.target(command, parameter, slot).is_err())
            })
            .collect()
    }

    /// The five move entries of the creature that chooses as the rows of the list of its
    /// body attacks show them: a known entry that costs more mana than the creature has
    /// is refused, as the row's test refuses it in a fight.
    pub fn attack_rows(&self) -> [AttackRow; attacks::ROWS] {
        std::array::from_fn(|row| {
            let Some(record) = self.chooser() else {
                return AttackRow::default();
            };
            let entry = record.moves[row];
            let effect = self.tables.body_attacks.get(usize::from(entry.attack));
            AttackRow {
                known: entry.known,
                attack: entry.attack,
                grade: entry.grade,
                // An attack past the table has no element.
                class: effect.map_or(u8::MAX, |effect| effect.class),
                refused: entry.known && self.attack(row) == Err(Some(text::NO_MANA)),
            }
        })
    }

    /// The four rows of the list of the spells of the creature that chooses, from the
    /// list's first line on: a spell that costs more mana than the creature has is
    /// short, as the row's test refuses it in a fight.
    pub fn spell_rows(&self, first: i16) -> SpellRows {
        let bits = self.chooser().map_or(0, |record| record.spells);
        let line = |row: usize| first.wrapping_add(row as i16);
        SpellRows {
            rows: std::array::from_fn(|row| spell_at(bits, line(row))),
            short: std::array::from_fn(|row| {
                let line = usize::try_from(line(row)).ok();
                line.is_some_and(|line| self.spell(line) == Err(Some(text::NO_MANA)))
            }),
            known: spells_known(bits),
        }
    }

    /// The mana the row under the cursor of a creature's list costs, which the list
    /// writes as a loss into the preview of the mana (`0x800C9352`): the entry's cost
    /// on the list of the body attacks (`0x80031A60`: 0 for an entry that is not
    /// known), the spell's on the list of the spells. Stand-in: 0 for a row without a
    /// spell, where the game reads the cost of spell 30, which is none.
    pub fn row_costs(&self, menu: &Menu) -> i16 {
        let Some(record) = self.chooser() else {
            return 0;
        };
        let drew = menu.drew();
        match drew.level {
            level::ATTACKS if drew.leaves_out & leaves_out::ATTACKS == 0 => {
                let row = usize::try_from(Menu::line(menu.attacks())).ok();
                let entry = row.and_then(|row| record.moves.get(row));
                entry
                    .filter(|entry| entry.known)
                    .map_or(0, |entry| entry.cost() as i16)
            }
            level::SPELLS if drew.leaves_out & leaves_out::SPELLS == 0 => {
                let spell = spell_at(record.spells, Menu::line(menu.spells()));
                self.tables
                    .spells
                    .get(spell)
                    .map_or(0, |spell| i16::from(spell.cost))
            }
            _ => 0,
        }
    }

    /// The last place in use of the list, -1 for an empty one.
    pub fn last_place(&self) -> i16 {
        self.last_held().map_or(-1, |last| last as i16)
    }

    /// The preview of the pass the menu did last (`0x800C8580`): what the command would
    /// change of the HP and of the mana of the fighter the box of the own side shows, a
    /// gain above 0. The resolver decides the command for that one target, with its
    /// draws, in every pass that previews; the actor's own share counts when the target
    /// acts himself and the command is no drain; each is cut to what the record lacks,
    /// and where the test of the target refuses the HP's is 0, for an item the mana's
    /// too.
    pub fn preview(&self, menu: &Menu, odd_frame: bool, draws: &mut impl Draws) -> (i16, i16) {
        let drew = menu.drew();
        let (fight, subject) = (self.fight, drew.subject);
        let record = |slot: usize| fight.records.get(slot).and_then(Option::as_ref);
        let (Some(actor), Some(target)) = (record(fight.fighter), record(subject)) else {
            return (0, 0);
        };
        if !drew.previews {
            return (0, 0);
        }
        let side = |slot: usize| match slot {
            PLAYER => Side::Player,
            _ => Side::Creature,
        };
        let parameter = menu.parameter() as usize;
        let order = Order {
            command: match menu.kind {
                0 => Command::Attack,
                1 => Command::Spell(parameter),
                4 => Command::Item(parameter),
                6 => Command::Body(parameter),
                _ => Command::Other,
            },
            actor,
            actor_side: side(fight.fighter),
            target,
            target_side: side(subject),
            deva: fight.deva,
            lock: fight.place_deva,
            odd_frame,
        };
        let (mut hp, mut mp) = (0i32, 0i32);
        if let Some(result) = resolve(self.tables, &order, draws) {
            if subject == fight.fighter && !drains(self.tables, &order) {
                hp -= result.actor_hp;
                mp -= result.cost + result.actor_mp;
            }
            hp -= result.hp;
            mp -= result.mp;
        }
        let lacks = |value: i16, most: i16| i32::from(most) - i32::from(value);
        let (mut hp, mut mp) = (
            hp.min(lacks(target.hp, target.max_hp)) as i16,
            mp.min(lacks(target.mp, target.max_mp)) as i16,
        );
        let command = menu.command();
        let refused = matches!(command, command::ITEM | command::BODY | command::SPELL)
            && (self.target(command, menu.parameter(), menu.picked())).is_err();
        if refused {
            hp = 0;
            if command == command::ITEM {
                mp = 0;
            }
        }
        (hp, mp)
    }
}

impl Asked for Lists<'_> {
    fn item(&self, place: usize) -> Result<Listed, Refused> {
        let Some((number, flags, pieces)) = self.at(place) else {
            return Err(None);
        };
        let count = pieces.min(255) as u8;
        match using::row_test(count, number, flags, true, false) {
            using::row::FINE => {}
            using::row::NEVER | using::row::NOT_IN_A_FIGHT => return Err(Some(text::NOT_HERE)),
            _ => return Err(None),
        }
        // The class of the item's flags: the side, and all of it at once.
        let side = match flags & item::AT_THE_OPPONENTS != 0 {
            true => class::OPPONENTS,
            false => class::OWN,
        };
        let everybody = match flags & item::ALL_OF_A_SIDE != 0 {
            true => class::EVERYBODY,
            false => 0,
        };
        Ok(Listed {
            parameter: number as u32,
            class: side | everybody,
        })
    }

    fn attack(&self, row: usize) -> Result<Listed, Refused> {
        // `0x80033CF4`: answer 7 for an entry that is not known, 5 for too little MP.
        let record = self.chooser().ok_or(None)?;
        let entry = (record.moves.get(row).filter(|entry| entry.known)).ok_or(None)?;
        if i32::from(record.mp) < entry.cost() {
            return Err(Some(text::NO_MANA));
        }
        // `0x80031B68`: attack 12 goes to every opponent at once, the others to one.
        let everybody = match entry.attack {
            ALL_AT_ONCE => class::EVERYBODY,
            _ => 0,
        };
        Ok(Listed {
            parameter: row as u32,
            class: class::OPPONENTS | everybody,
        })
    }

    fn spell(&self, row: usize) -> Result<Listed, Refused> {
        // `0x80033C64`: answer 6 behind the last spell known, 5 for too little MP.
        let record = self.chooser().ok_or(None)?;
        let row = i16::try_from(row).map_err(|_| None)?;
        let number = crate::player_panel::spell_at(record.spells, row);
        if number >= crate::player_panel::spells::SPELLS {
            return Err(None);
        }
        let spell = self.tables.spells.get(number).ok_or(None)?;
        if record.mp < i16::from(spell.cost) {
            return Err(Some(text::NO_MANA));
        }
        // `0x8001ACE4`: bit 6 of the spell's word is the own side, bit 7 the
        // opponents, bit 2 all of the side at once.
        let bit = |bit: u32, class: u8| match spell.word >> bit & 1 {
            0 => 0,
            _ => class,
        };
        Ok(Listed {
            parameter: number as u32,
            class: bit(6, class::OWN) | bit(7, class::OPPONENTS) | bit(2, class::EVERYBODY),
        })
    }

    fn own(&self, command: u8, _: u32, row: i16) -> usize {
        let fighter = self.fight.fighter;
        let row = usize::try_from(row).unwrap_or(0);
        let mut carried = self.carried();
        let slot = match command {
            // The player's row comes first; a change leaves the acting slot out.
            command::ITEM => row.checked_sub(1).map_or(Some(0), |n| carried.nth(n)),
            command::CHANGE => row
                .checked_sub(1)
                .map_or(Some(0), |n| carried.filter(|slot| *slot != fighter).nth(n)),
            command::SPELL => Some(fighter),
            _ => carried.nth(row),
        };
        slot.unwrap_or(0)
    }

    fn target(&self, command: u8, parameter: u32, picked: usize) -> Result<(), Refused> {
        let Some(record) = self.fight.records.get(picked).and_then(Option::as_ref) else {
            return Ok(());
        };
        match command {
            command::ITEM => {
                let number = parameter as usize;
                let flags = self.tables.items.get(number).map_or(0, |item| item.flags);
                match using::aim_test(number, flags, picked, record) {
                    using::aim::NOT_FOR_CREATURES => Err(Some(text::NOT_FOR_CREATURES)),
                    using::aim::NOT_FOR_THE_PLAYER => Err(Some(text::FOR_CREATURES)),
                    using::aim::NO_NEED => Err(Some(text::NO_NEED)),
                    using::aim::DEAD => Err(Some(text::CANNOT_COME)),
                    _ => Ok(()),
                }
            }
            command::SUMMON | command::CHANGE if record.is_dead() => Err(Some(text::CANNOT_COME)),
            // `0x800340C4`: only a spell for the own side is tested, by byte 7 of its
            // record: 0, the target lacks HP; 1, the spell is number 16 or the target
            // has a status of the bits `0x7FB`. Else there is no need for it.
            command::SPELL => {
                let Some(spell) = self.tables.spells.get(parameter as usize) else {
                    return Ok(());
                };
                let needed = match spell.need {
                    0 => record.hp != record.max_hp,
                    1 => parameter == CURES_ALWAYS || record.status & CURABLE != 0,
                    _ => true,
                };
                match spell.word & FOR_THE_OWN_SIDE == 0 || needed {
                    true => Ok(()),
                    false => Err(Some(text::NO_NEED)),
                }
            }
            _ => Ok(()),
        }
    }

    fn described(&self, level: u8, line: usize) -> bool {
        match level {
            level::ITEMS => self.at(line).is_some(),
            // `0x800C7BEC`: the spell of the line among those the creature knows.
            level::SPELLS => self.chooser().is_some_and(|record| {
                let line = i16::try_from(line).unwrap_or(i16::MAX);
                spell_at(record.spells, line) < spells::SPELLS
            }),
            _ => false,
        }
    }
}

/// The command a finished menu hands over (`0x800C8B40`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chosen {
    pub kind: u32,
    pub parameter: u32,
    pub actors: u8,
    pub targets: u8,
    /// The lowest slot of the menu's own target mask, which a flight, a defence and a
    /// change do not hand on as their targets.
    pub picked: Option<usize>,
}

impl Chosen {
    /// The choice of the rules; `None` for a command without the slot it needs.
    pub fn choice(&self) -> Option<Choice> {
        let (targets, parameter) = (self.targets, self.parameter as usize);
        Some(match self.kind {
            0 => Choice::Attack { targets },
            1 => Choice::Spell {
                spell: parameter,
                targets,
            },
            2 => Choice::Summon { slot: self.picked? },
            4 => Choice::Item {
                item: parameter,
                targets,
            },
            5 => Choice::Capture { targets },
            6 => Choice::Body {
                slot: parameter,
                targets,
            },
            7 => Choice::Flee,
            8 => Choice::Change { slot: self.picked? },
            18 => Choice::Defend,
            _ => return None,
        })
    }
}

/// What a pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pass {
    /// The sounds, in the order they are asked for.
    pub sounds: Vec<u32>,
    /// The message this pass draws.
    pub message: Option<u8>,
    /// The marked opponents changed (and in the first pass): the fighter turns to them
    /// and the camera is asked with their mask.
    pub marked: Option<u8>,
    /// The menu is done: the task ends in this pass.
    pub chosen: Option<Chosen>,
}

/// What a pass drew. The drawing stands in front of the input in a pass, so the level
/// and the half word are those the pass began with, and a message is first drawn in the
/// pass behind the confirm that was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Drew {
    pub level: u8,
    /// What the pass left out ([`leaves_out`]).
    pub leaves_out: u16,
    /// The description of what is under the cursor was drawn.
    pub describes: bool,
    /// The message of a refused confirm that the pass's state drew.
    pub message: Option<u8>,
    /// The pass drew the picker of a target on the own side, in place of the root: the
    /// row it was handed as its cursor, -1 for a class that picks nobody.
    pub picks: Option<i16>,
    /// The marks of the HUD as the pass left them (`0x800C75F4`, by the cursor the pass
    /// began with; the pass that hands the command over clears them): the opponents by
    /// their numbers, and the own side.
    pub marks: (u8, bool),
    /// The fighter the box of the own side shows (`0x800C7F24`): the one that chooses,
    /// or on a target of the own side alone the slot the picker answered.
    pub subject: usize,
    /// The preview ran for the subject (`0x800C8580`): a target, with the subject in
    /// the target mask.
    pub previews: bool,
}

/// A command menu from its making to its last pass.
#[derive(Debug, Clone)]
pub struct Menu {
    offer: Offer,
    level: u8,
    /// The level a cancel goes back to.
    back: u8,
    class: u8,
    kind: u32,
    parameter: u32,
    /// The slot of the own side the picker answered last.
    picked: usize,
    targets: u8,
    /// A message, its count, and whether its state is on.
    message: Option<u8>,
    count: u8,
    up: bool,
    describes: bool,
    openness: u8,
    drew: Drew,
    marked: Option<u8>,
    root: Cursor,
    items: Cursor,
    attacks: Cursor,
    spells: Cursor,
    opponents: Cursor,
    own: Cursor,
    everybody: Cursor,
}

impl Menu {
    /// The maker (`0x800C70CC`): every cursor on its first cell but the one among the
    /// opponents, which is kept while as many stand as it names.
    pub fn new(offer: Offer) -> Self {
        let list = |rows: i16, lines: u16, flags: u16| Cursor {
            columns: 1,
            rows,
            lines,
            flags,
            at: 0,
            first: 0,
        };
        let standing = offer.opponents.count_ones() as i16;
        let lines = if offer.fighter == 0 { 6 } else { 5 };
        Self {
            offer,
            level: level::ROOT,
            back: level::ROOT,
            class: class::OPPONENTS,
            kind: 0,
            parameter: 0,
            picked: 0,
            targets: 0,
            message: None,
            count: 0,
            up: false,
            describes: false,
            openness: 0,
            drew: Drew {
                subject: offer.fighter,
                ..Drew::default()
            },
            marked: None,
            root: Cursor {
                columns: 2,
                ..list(3, lines, 0)
            },
            items: list(4, offer.places.max(4), cursor::SCROLLS),
            attacks: list(5, 5, 0),
            spells: list(4, offer.spells.max(4), cursor::SCROLLS),
            opponents: Cursor {
                columns: standing,
                at: if standing - 1 < offer.kept {
                    0
                } else {
                    offer.kept
                },
                ..list(1, standing as u16, 0)
            },
            own: list(0, 0, 0),
            everybody: list(1, 1, 0),
        }
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    /// 0 the player's menu, 1 his creature's.
    pub fn side(&self) -> usize {
        usize::from(self.offer.fighter != 0)
    }

    /// The command of the root cell under the cursor (`0x800C7AA0`).
    pub fn command(&self) -> u8 {
        let cell = (self.root.at + self.root.first).clamp(0, 5) as usize;
        COMMANDS[self.side()][cell]
    }

    /// The command of a cell of the root menu.
    pub fn command_of(&self, cell: usize) -> Option<u8> {
        COMMANDS[self.side()].get(cell).copied()
    }

    /// What the last pass drew.
    pub fn drew(&self) -> Drew {
        self.drew
    }

    /// The cursor record of the level (`0x800C7588`).
    pub fn cursor(&self) -> &Cursor {
        match self.level {
            level::ITEMS => &self.items,
            level::ATTACKS => &self.attacks,
            level::SPELLS => &self.spells,
            level::TARGET if self.class & class::EVERYBODY != 0 => &self.everybody,
            level::TARGET if self.class & class::OPPONENTS != 0 => &self.opponents,
            level::TARGET => &self.own,
            _ => &self.root,
        }
    }

    fn cursor_mut(&mut self) -> &mut Cursor {
        match self.level {
            level::ITEMS => &mut self.items,
            level::ATTACKS => &mut self.attacks,
            level::SPELLS => &mut self.spells,
            level::TARGET if self.class & class::EVERYBODY != 0 => &mut self.everybody,
            level::TARGET if self.class & class::OPPONENTS != 0 => &mut self.opponents,
            level::TARGET => &mut self.own,
            _ => &mut self.root,
        }
    }

    /// The cursor of the list of the items.
    pub fn items(&self) -> &Cursor {
        &self.items
    }

    /// The cursors of the list of the body attacks and of the list of the spells.
    pub fn attacks(&self) -> &Cursor {
        &self.attacks
    }

    pub fn spells(&self) -> &Cursor {
        &self.spells
    }

    /// The cursor of the picker of a target on the own side, with its rows.
    pub fn own(&self) -> &Cursor {
        &self.own
    }

    /// The parameter of the command: the item, the body attack or the spell of the row
    /// the last list was left on.
    pub fn parameter(&self) -> u32 {
        self.parameter
    }

    /// The root's cursor, which the root menu is drawn with on every level.
    pub fn root(&self) -> &Cursor {
        &self.root
    }

    /// The openness of the root menu: one more a pass, up to 8.
    pub fn openness(&self) -> u8 {
        self.openness
    }

    /// The root commands that are drawn dim, by their numbers.
    pub fn closed(&self, command: u8) -> Option<u8> {
        if self.side() != 0 {
            return None;
        }
        match command {
            command::SUMMON if self.offer.carried == 0 => Some(text::NO_CREATURE),
            command::CAPTURE if self.offer.boss => Some(text::CANNOT_CAPTURE),
            command::CAPTURE if self.offer.no_cocoon => Some(text::NO_COCOON),
            command::FLEE if self.offer.no_retreat => Some(text::CANNOT_FLEE),
            _ => None,
        }
    }

    /// Whether the last pass drew the description of what is under the cursor.
    pub fn describes(&self) -> bool {
        self.describes
    }

    /// The half word of the last pass that says what it leaves out ([`leaves_out`]).
    pub fn draws(&self) -> u16 {
        DRAWS[usize::from(self.level)][usize::from(self.up || self.describes)]
    }

    /// The slot of the own side the picker answered last, and the target mask of the
    /// command as the last pass made it.
    pub fn picked(&self) -> usize {
        self.picked
    }

    pub fn targets(&self) -> u8 {
        self.targets
    }

    /// The cursor among the opponents, for the next menu's [`Offer::kept`].
    pub fn kept(&self) -> i16 {
        self.opponents.at
    }

    /// The message of a refused confirm while its state is on.
    pub fn message(&self) -> Option<u8> {
        self.message.filter(|_| self.up)
    }

    /// The line under a cursor (`0x8003C7EC`).
    fn line(cursor: &Cursor) -> i16 {
        cursor.at.wrapping_add(cursor.first)
    }

    /// The slots of the opponents on the field, in the order of their numbers.
    fn standing(&self) -> impl Iterator<Item = usize> + '_ {
        (OPPONENTS..SLOTS).filter(|slot| self.offer.opponents >> slot & 1 != 0)
    }

    /// The mask of the opponent under the cursor.
    fn pointed(&self) -> u8 {
        let at = usize::try_from(Self::line(&self.opponents)).unwrap_or(0);
        self.standing().nth(at).map_or(0, |slot| 1 << slot)
    }

    /// The marks of the HUD (`0x800C75F4`): on a target the opponent under the cursor,
    /// or the own side; a command on everybody marks all three opponents, the own
    /// side, or both, by its class.
    fn marks(&self) -> (u8, bool) {
        if self.level != level::TARGET {
            return (0, false);
        }
        let (opponents, own) = (
            self.class & class::OPPONENTS != 0,
            self.class & class::OWN != 0,
        );
        if self.class & class::EVERYBODY != 0 {
            return (if opponents { 0b111 } else { 0 }, own);
        }
        match opponents {
            true => (self.pointed() >> OPPONENTS, false),
            false => (0, true),
        }
    }

    /// The target mask of the command (`0x800C7AEC`).
    fn mask(&self) -> u8 {
        if self.class & class::EVERYBODY != 0 {
            let opponents = match self.class & class::OPPONENTS != 0 {
                true => self.offer.opponents,
                false => 0,
            };
            return match self.class & class::OWN != 0 {
                true => opponents | 1 | self.offer.carried,
                false => opponents,
            };
        }
        match self.class & class::OPPONENTS != 0 {
            true => self.pointed(),
            false => 1u8.checked_shl(self.picked as u32).unwrap_or(0),
        }
    }

    /// One pass of the task's routine (`0x800C8BD8`) with the pad's words.
    pub fn pass(&mut self, pad: &Pad, asked: &impl Asked) -> Pass {
        let mut pass = Pass::default();
        if self.level == level::DONE {
            return pass;
        }
        // The marks of the HUD go by the cursor as the pass finds it.
        let marks = self.marks();
        // The cursor moves in every pass, also under a message.
        if (self.cursor_mut().step_with(pad.repeated, pad.held)).sounds {
            pass.sounds.push(sound::MOVED);
        }
        self.openness = (self.openness + 1).min(OPEN);
        // The drawing: the description on the held circle, and the picker of the own
        // side, which answers the slot under its cursor.
        let line = usize::try_from(Self::line(self.cursor())).unwrap_or(0);
        self.describes = pad.held & pad::CIRCLE != 0
            && !self.up
            && match self.level {
                level::ROOT | level::ATTACKS => true,
                level::ITEMS | level::SPELLS => asked.described(self.level, line),
                _ => false,
            };
        self.drew = Drew {
            level: self.level,
            leaves_out: self.draws(),
            describes: self.describes,
            message: None,
            picks: None,
            marks,
            subject: self.offer.fighter,
            previews: false,
        };
        let command = self.command();
        if self.draws() & leaves_out::PICKER == 0 {
            let picks = match command {
                command::ITEM | command::SPELL => self.class & class::OWN != 0,
                command::SUMMON | command::CHANGE => true,
                _ => false,
            };
            let row = match self.class & class::EVERYBODY != 0 {
                true => -1,
                false => Self::line(self.cursor()),
            };
            if picks {
                self.picked = asked.own(command, self.parameter, row);
                self.drew.picks = Some(row);
            }
        }
        self.targets = self.mask();
        // The subject of the box of the own side, and whether the preview runs for it.
        if self.level == level::TARGET {
            if self.class & (class::EVERYBODY | class::OPPONENTS) == 0 {
                self.drew.subject = self.picked;
            }
            let subject = self.drew.subject;
            self.drew.previews = subject < OPPONENTS && self.targets >> subject & 1 != 0;
        }
        // The marked: the opponent under the cursor, else all of them.
        let single = self.level == level::TARGET
            && self.class & (class::EVERYBODY | class::OPPONENTS) == class::OPPONENTS;
        let marked = if single {
            self.pointed()
        } else {
            self.offer.opponents
        };
        if self.marked != Some(marked) {
            self.marked = Some(marked);
            pass.marked = Some(marked);
        }
        match self.up {
            true => self.shows(pad, &mut pass),
            false => self.input(pad, asked, &mut pass),
        }
        self.drew.message = pass.message;
        if self.level == level::DONE {
            // The marks and the preview are cleared with the command handed over.
            (self.drew.marks, self.drew.previews) = ((0, false), false);
            pass.chosen = Some(self.chosen());
        }
        pass
    }

    /// The state of a message (`0x800C8AB4`): any button ends its count.
    fn shows(&mut self, pad: &Pad, pass: &mut Pass) {
        if pad.pressed != 0 {
            self.count = 0;
        }
        match self.count {
            0 => self.up = false,
            _ => self.count -= 1,
        }
        pass.message = self.message;
    }

    /// The state of the input (`0x800C8938`).
    fn input(&mut self, pad: &Pad, asked: &impl Asked, pass: &mut Pass) {
        if menu::confirmed(pad) {
            match self.test(asked) {
                Ok(listed) => {
                    pass.sounds.push(sound::CONFIRMED);
                    self.step(listed);
                }
                Err(message) => {
                    pass.sounds.push(sound::REFUSED);
                    self.message = message;
                    if message.is_some() {
                        (self.count, self.up) = (MESSAGE, true);
                    }
                }
            }
        } else if menu::cancelled(pad) && self.level != level::ROOT {
            pass.sounds.push(sound::CANCELLED);
            (self.level, self.back) = (self.back, level::ROOT);
        }
    }

    /// The test of a confirm (`0x800C7700`).
    fn test(&self, asked: &impl Asked) -> Result<Option<Listed>, Refused> {
        let line = usize::try_from(Self::line(self.cursor())).unwrap_or(0);
        let command = self.command();
        match self.level {
            level::ROOT => match self.closed(command) {
                Some(message) => Err(Some(message)),
                None => Ok(None),
            },
            level::ITEMS => asked.item(line).map(Some),
            level::ATTACKS => asked.attack(line).map(Some),
            level::SPELLS => asked.spell(line).map(Some),
            level::TARGET => match command {
                command::ITEM
                | command::BODY
                | command::SPELL
                | command::SUMMON
                | command::CHANGE => asked
                    .target(command, self.parameter, self.picked)
                    .map(|()| None),
                _ => Ok(None),
            },
            _ => Ok(None),
        }
    }

    /// The step of a confirm that passed (`0x800C7C3C`).
    fn step(&mut self, listed: Option<Listed>) {
        let command = usize::from(self.command());
        let from = self.level;
        self.back = from;
        match (from, listed) {
            (level::ROOT, _) => {
                self.class = CLASS[command];
                self.level = NEXT[command];
                self.kind = KIND[command];
            }
            (level::ITEMS | level::ATTACKS | level::SPELLS, Some(listed)) => {
                self.class = listed.class;
                self.parameter = listed.parameter;
                self.level = level::TARGET;
            }
            (level::TARGET, _) => {
                self.level = level::DONE;
                self.targets = self.mask();
            }
            _ => {}
        }
        if self.level == level::TARGET {
            // The rows of the own side: the carried creatures, for an item with the
            // player; a spell has one.
            let carried = self.offer.carried.count_ones() as i16;
            let rows = match command as u8 {
                command::ITEM => Some(carried + 1),
                command::SUMMON | command::CHANGE => Some(carried),
                command::SPELL => Some(1),
                _ => None,
            };
            if let Some(rows) = rows {
                (self.own.rows, self.own.lines) = (rows, rows as u16);
            }
            (self.own.at, self.own.first) = (0, 0);
        }
    }

    /// The command as it is handed over (`0x800C8B40`): a flight and a defence go
    /// against every opponent on the field, a change names no target.
    fn chosen(&self) -> Chosen {
        Chosen {
            kind: self.kind,
            parameter: self.parameter,
            actors: 1 << self.offer.fighter,
            targets: match self.kind {
                7 | 18 => self.offer.opponents,
                8 => 0,
                _ => self.targets,
            },
            picked: (self.targets != 0).then(|| self.targets.trailing_zeros() as usize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::Repeat;

    /// Lists of a made-up fight: the item of a place is its number, place 2 is empty,
    /// item 1 is for the own side and item 3 cannot be used; the second attack costs
    /// too much; the carried creature of slot 2 is dead.
    struct Lists;

    impl Asked for Lists {
        fn item(&self, place: usize) -> Result<Listed, Refused> {
            match place {
                2 => Err(None),
                3 => Err(Some(text::NOT_HERE)),
                1 => Ok(Listed {
                    parameter: 1,
                    class: class::OWN,
                }),
                _ => Ok(Listed {
                    parameter: place as u32,
                    class: class::OPPONENTS,
                }),
            }
        }

        fn attack(&self, row: usize) -> Result<Listed, Refused> {
            match row {
                1 => Err(Some(text::NO_MANA)),
                _ => Ok(Listed {
                    parameter: row as u32,
                    class: class::OPPONENTS,
                }),
            }
        }

        fn spell(&self, row: usize) -> Result<Listed, Refused> {
            Ok(Listed {
                parameter: 20 + row as u32,
                class: class::EVERYBODY | class::OPPONENTS,
            })
        }

        fn own(&self, command: u8, _: u32, row: i16) -> usize {
            // An item's rows start with the player, a summon's with the first creature.
            match command {
                command::ITEM => row.max(0) as usize,
                _ => row.max(0) as usize + 1,
            }
        }

        fn target(&self, _: u8, _: u32, picked: usize) -> Result<(), Refused> {
            match picked {
                2 => Err(Some(text::CANNOT_COME)),
                _ => Ok(()),
            }
        }

        fn described(&self, _: u8, line: usize) -> bool {
            line != 2
        }
    }

    fn offer(fighter: usize) -> Offer {
        Offer {
            fighter,
            carried: 0b0110,
            opponents: 0b0101_0000,
            boss: false,
            no_cocoon: true,
            no_retreat: true,
            places: 6,
            spells: 2,
            kept: 0,
        }
    }

    /// A pad whose buttons went down in this pass.
    fn pressed(buttons: u16) -> Pad {
        let mut pad = Pad::default();
        pad.read(buttons, Repeat::of(2), 0);
        pad
    }

    fn go(menu: &mut Menu, buttons: u16) -> Pass {
        menu.pass(&pressed(buttons), &Lists)
    }

    #[test]
    fn an_attack_is_a_confirm_on_the_root_and_one_on_an_opponent() {
        let mut menu = Menu::new(offer(0));
        assert_eq!(
            (menu.root().columns, menu.root().rows, menu.root().lines),
            (2, 3, 6)
        );
        // The first pass marks every opponent, for the camera and the turn.
        let first = go(&mut menu, 0);
        assert_eq!((first.marked, first.chosen), (Some(0b0101_0000), None));
        assert_eq!((menu.openness(), menu.command()), (1, command::ATTACK));
        let pass = go(&mut menu, pad::CROSS);
        assert_eq!(pass.sounds, [sound::CONFIRMED]);
        assert_eq!(menu.level(), level::TARGET);
        // The root marks nothing for the HUD.
        assert_eq!(menu.drew().marks, (0, false));
        // On the target the opponent under the cursor is marked: the first that stands.
        assert_eq!(go(&mut menu, 0).marked, Some(1 << 4));
        // The HUD's mark is opponent 0, by the cursor the pass began with; the box of
        // the own side stays the fighter's, and nothing is previewed for him.
        let drew = menu.drew();
        assert_eq!(
            (drew.marks, drew.subject, drew.previews),
            ((1, false), 0, false)
        );
        // Right goes to the other one, which is opponent 2; the edge stops the cursor.
        let pass = go(&mut menu, pad::RIGHT);
        assert_eq!(
            (pass.sounds.as_slice(), pass.marked),
            ([sound::MOVED].as_slice(), Some(1 << 6))
        );
        // The pass of the move still marked the first; the one behind it marks the other.
        assert_eq!(menu.drew().marks.0, 1);
        assert_eq!(go(&mut menu, pad::RIGHT).sounds, [] as [u32; 0]);
        assert_eq!(menu.drew().marks.0, 1 << 2);
        let chosen = go(&mut menu, pad::CROSS).chosen.unwrap();
        assert_eq!(menu.drew().marks, (0, false));
        assert_eq!((chosen.kind, chosen.actors, chosen.targets), (0, 1, 1 << 6));
        assert_eq!(chosen.choice(), Some(Choice::Attack { targets: 1 << 6 }));
        // The cursor among the opponents is what the next menu starts with.
        assert_eq!(menu.kept(), 1);
        let again = Menu::new(Offer {
            kept: 1,
            ..offer(0)
        });
        assert_eq!(again.kept(), 1);
        let fewer = Menu::new(Offer {
            kept: 1,
            opponents: 1 << 5,
            ..offer(0)
        });
        assert_eq!(fewer.kept(), 0);
        // A finished menu does nothing more.
        assert_eq!(go(&mut menu, pad::CROSS), Pass::default());
    }

    #[test]
    fn a_defence_is_chosen_in_the_pass_of_its_confirm() {
        let mut menu = Menu::new(offer(0));
        // Right and the cross in the first pass: nothing waits for the root to open.
        let pass = go(&mut menu, pad::RIGHT | pad::CROSS);
        assert_eq!(pass.sounds, [sound::MOVED, sound::CONFIRMED]);
        let chosen = pass.chosen.unwrap();
        // A defence goes against every opponent on the field.
        assert_eq!((chosen.kind, chosen.targets), (18, 0b0101_0000));
        assert_eq!(chosen.choice(), Some(Choice::Defend));
    }

    #[test]
    fn a_closed_command_refuses_with_its_message() {
        let mut menu = Menu::new(offer(0));
        assert_eq!(menu.closed(command::CAPTURE), Some(text::NO_COCOON));
        assert_eq!(menu.closed(command::SUMMON), None);
        let boss = Menu::new(Offer {
            boss: true,
            ..offer(0)
        });
        assert_eq!(boss.closed(command::CAPTURE), Some(text::CANNOT_CAPTURE));
        // Down and right: the cell of the capture.
        go(&mut menu, pad::DOWN | pad::RIGHT);
        assert_eq!(menu.command(), command::CAPTURE);
        let pass = go(&mut menu, pad::CROSS);
        assert_eq!(
            (pass.sounds.as_slice(), pass.message),
            ([sound::REFUSED].as_slice(), None)
        );
        // The message is drawn from the next pass on, the cursor moves under it, and a
        // confirm is not asked while it is up.
        let pass = go(&mut menu, pad::DOWN);
        assert_eq!(pass.message, Some(text::NO_COCOON));
        assert_eq!(
            (pass.sounds.as_slice(), menu.command()),
            ([sound::MOVED].as_slice(), command::FLEE)
        );
        // That button ended the count: the pass that saw it was the message's last.
        assert_eq!(go(&mut menu, 0).message, None);
        // Without a button it stays 150 passes behind the one that set it up.
        let pass = go(&mut menu, pad::CROSS);
        assert_eq!(pass.sounds, [sound::REFUSED]);
        let shown = (0..200)
            .filter(|_| go(&mut menu, 0).message.is_some())
            .count();
        assert_eq!(shown, 151);
        assert_eq!(menu.level(), level::ROOT);
    }

    #[test]
    fn a_cancel_goes_back_one_level_and_does_nothing_on_the_root() {
        let mut menu = Menu::new(offer(0));
        assert_eq!(go(&mut menu, pad::TRIANGLE).sounds, [] as [u32; 0]);
        // Down twice: the items.
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        assert_eq!(menu.command(), command::ITEM);
        go(&mut menu, pad::CROSS);
        assert_eq!((menu.level(), menu.cursor().lines), (level::ITEMS, 6));
        // Place 2 is empty: refused without a message. Place 3 has one.
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        let pass = go(&mut menu, pad::CROSS);
        assert_eq!(pass.sounds, [sound::REFUSED]);
        assert_eq!(go(&mut menu, 0).message, None);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        assert_eq!(go(&mut menu, pad::UP).message, Some(text::NOT_HERE));
        go(&mut menu, 0);
        // Item 0 goes to an opponent; a cancel there leads back to the list, the next
        // one to the root.
        go(&mut menu, pad::UP);
        go(&mut menu, pad::UP);
        go(&mut menu, pad::CROSS);
        assert_eq!(menu.level(), level::TARGET);
        assert_eq!(go(&mut menu, pad::TRIANGLE).sounds, [sound::CANCELLED]);
        assert_eq!(menu.level(), level::ITEMS);
        go(&mut menu, pad::TRIANGLE);
        assert_eq!(menu.level(), level::ROOT);
    }

    #[test]
    fn an_item_for_the_own_side_goes_to_the_slot_the_picker_answers() {
        let mut menu = Menu::new(offer(0));
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        // The player and the two carried creatures.
        assert_eq!((menu.level(), menu.cursor().rows), (level::TARGET, 3));
        assert_eq!(menu.draws() & leaves_out::PICKER, 0);
        go(&mut menu, pad::DOWN);
        assert_eq!((menu.picked(), menu.targets()), (1, 1 << 1));
        // The box of the own side shows the picked, marked, and the preview is his.
        let drew = menu.drew();
        assert_eq!(
            (drew.marks, drew.subject, drew.previews),
            ((0, true), 1, true)
        );
        // The creature of slot 2 is dead.
        go(&mut menu, pad::DOWN);
        assert_eq!(go(&mut menu, pad::CROSS).sounds, [sound::REFUSED]);
        assert_eq!(go(&mut menu, pad::UP).message, Some(text::CANNOT_COME));
        go(&mut menu, 0);
        let chosen = go(&mut menu, pad::CROSS).chosen.unwrap();
        assert_eq!(
            chosen.choice(),
            Some(Choice::Item {
                item: 1,
                targets: 1 << 1
            })
        );
    }

    #[test]
    fn a_creature_has_five_cells_and_its_change_names_no_target() {
        let mut menu = Menu::new(offer(1));
        assert_eq!((menu.side(), menu.root().lines), (1, 5));
        // Nothing of a creature's root is closed.
        assert_eq!(menu.closed(command::FLEE), None);
        // The spells go to every opponent at once: one cell, nothing to pick.
        go(&mut menu, pad::DOWN | pad::RIGHT);
        assert_eq!(menu.command(), command::SPELL);
        go(&mut menu, pad::CROSS);
        assert_eq!((menu.level(), menu.cursor().lines), (level::SPELLS, 4));
        go(&mut menu, pad::CROSS);
        assert_eq!((menu.level(), menu.cursor().lines), (level::TARGET, 1));
        // A command on every opponent marks all three for the HUD.
        go(&mut menu, 0);
        assert_eq!(menu.drew().marks, (0b111, false));
        let chosen = go(&mut menu, pad::CROSS).chosen.unwrap();
        assert_eq!(
            chosen.choice(),
            Some(Choice::Spell {
                spell: 20,
                targets: 0b0101_0000
            })
        );
        // The second attack costs too much; the change picks a carried creature.
        let mut menu = Menu::new(offer(1));
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        assert_eq!(go(&mut menu, 0).message, Some(text::NO_MANA));
        let mut menu = Menu::new(offer(1));
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        assert_eq!(menu.command(), command::CHANGE);
        go(&mut menu, pad::CROSS);
        assert_eq!(menu.cursor().rows, 2);
        let chosen = go(&mut menu, pad::CROSS).chosen.unwrap();
        assert_eq!(
            (chosen.kind, chosen.targets, chosen.picked),
            (8, 0, Some(1))
        );
        assert_eq!(chosen.choice(), Some(Choice::Change { slot: 1 }));
    }

    #[test]
    fn the_held_circle_describes_and_takes_the_root_away_on_a_list() {
        let mut menu = Menu::new(offer(0));
        let mut held = Pad::default();
        held.read(pad::CIRCLE, Repeat::of(2), 0);
        held.read(pad::CIRCLE, Repeat::of(2), 1);
        menu.pass(&held, &Lists);
        assert!(menu.describes());
        assert_eq!(menu.draws() & leaves_out::ROOT, 0);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::CROSS);
        menu.pass(&held, &Lists);
        assert!(menu.describes());
        assert_eq!(
            menu.draws() & (leaves_out::ROOT | leaves_out::ITEMS),
            leaves_out::ROOT
        );
        // The empty place has nothing to describe.
        go(&mut menu, pad::DOWN);
        go(&mut menu, pad::DOWN);
        menu.pass(&held, &Lists);
        assert!(!menu.describes());
    }
}
