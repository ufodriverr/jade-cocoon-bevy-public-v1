
use std::fmt;

use crate::action::{ActionError, Head};
use crate::animation::{Animation, AnimationError};
use crate::mesh::{MeshBlock, MeshError};
use crate::rooms::{FIELD_ENGINE_BASE, Image, RoomTableError, step};
use crate::scene::Actor;
use crate::tim::{self, Tim};

/// RAM address of the model table: one pointer per model, ended by a null.
pub const MODEL_TABLE: u32 = 0x8008_23F0;
/// Bytes of a sector, the unit a record measures a model file in.
pub const SECTOR: usize = 2048;
/// More models than this is not a table.
const MAX_MODELS: usize = 1024;
/// RAM address of the species table of the executable: four bytes per species, the model
/// number in the first two.
pub const SPECIES_TABLE: u32 = 0x8007_BC54;

/// The model of a species, from the species table of the executable (`image` loaded at
/// `base`). `None` when the record lies outside the image; the table has no count.
pub fn species_model(image: &[u8], base: u32, species: u32) -> Option<u16> {
    let offset = SPECIES_TABLE
        .checked_sub(base)?
        .checked_add(species.checked_mul(4)?)? as usize;
    let bytes = image.get(offset..)?.get(..2)?;
    Some(u16::from_le_bytes(bytes.try_into().ok()?))
}

pub fn actor_model(image: &[u8], base: u32, actor: &Actor) -> Option<u32> {
    match actor.creature {
        Some(creature) => species_model(image, base, creature.species).map(u32::from),
        None => Some(actor.record.model),
    }
}

pub fn actor_turn(image: &[u8], base: u32, actor: &Actor) -> Option<i32> {
    let own = i32::from(actor.record.turn);
    let Some(creature) = actor.creature else {
        return Some(own);
    };
    Some(species_turn(image, base, creature.species)? + own)
}

pub fn species_turn(image: &[u8], base: u32, species: u32) -> Option<i32> {
    let offset = SPECIES_TABLE
        .checked_sub(base)?
        .checked_add(species.checked_mul(4)?)? as usize;
    let half = *image.get(offset.checked_add(2)?)?;
    Some(2 * i32::from(half))
}

/// The bytes +2 and +3 of a species' record in the species table: what the turn of the
/// colours of a carried creature's figure is worked out from (`creatures::colour_turn`).
pub fn species_turns(image: &[u8], base: u32, species: u32) -> Option<[u8; 2]> {
    let offset = SPECIES_TABLE
        .checked_sub(base)?
        .checked_add(species.checked_mul(4)?)? as usize;
    let bytes = image.get(offset.checked_add(2)?..)?.get(..2)?;
    bytes.try_into().ok()
}

/// Why the table or a piece of a model file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// A pointer or a record leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
    /// The table has no end within a sane number of models.
    NoEnd,
    /// A record names sectors that the model file does not have.
    OutOfFile(Sectors),
    /// The mesh section does not hold a mesh block.
    Mesh(MeshError),
    /// The texture section does not hold a texture set.
    Textures,
    /// The model has no animation of this number.
    NoAnimation(usize),
    /// An animation's sectors do not hold an animation block.
    Animation(AnimationError),
    /// The head section is not a block with an action table.
    Head(ActionError),
    /// The model has no animation set of this number.
    NoSet(usize),
    /// A set's sectors do not hold an animation set.
    Set,
    /// The model has no part of this number.
    NoPart(usize),
    /// A part's sectors do not hold a part.
    Part,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => {
                write!(f, "model table leads outside the image, to {address:#010x}")
            }
            Self::NoEnd => write!(f, "model table has no end"),
            Self::OutOfFile(sectors) => write!(
                f,
                "the model file has no {} sectors from sector {}",
                sectors.count, sectors.start
            ),
            Self::Mesh(err) => write!(f, "mesh section: {err}"),
            Self::Textures => write!(f, "the texture section is not a texture set"),
            Self::NoAnimation(number) => write!(f, "the model has no animation {number}"),
            Self::Animation(err) => write!(f, "animation: {err}"),
            Self::Head(err) => write!(f, "head section: {err}"),
            Self::NoSet(number) => write!(f, "the model has no animation set {number}"),
            Self::Set => write!(f, "the sectors of the set are not an animation set"),
            Self::NoPart(number) => write!(f, "the model has no part {number}"),
            Self::Part => write!(f, "the sectors of the part are not a part"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<RoomTableError> for ModelError {
    fn from(err: RoomTableError) -> Self {
        let RoomTableError::OutOfImage(address) = err;
        Self::OutOfImage(address)
    }
}

/// A run of sectors of a model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Sectors {
    pub start: u16,
    pub count: u16,
}

impl Sectors {
    /// The sector behind the run.
    pub fn end(self) -> usize {
        self.start as usize + self.count as usize
    }

    /// The run's bytes in the model file.
    pub fn bytes(self, file: &[u8]) -> Result<&[u8], ModelError> {
        file.get(self.start as usize * SECTOR..self.end() * SECTOR)
            .ok_or(ModelError::OutOfFile(self))
    }
}

/// The four sections every model file starts with, in file order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// A size and that many bytes the game keeps as they are: the action table and two
    /// more pieces ([`crate::action::Head`]).
    Head,
    /// The model's texture set.
    Textures,
    /// The model's mesh block.
    Mesh,
    /// The animations the game keeps in memory with the model; one empty sector when none.
    Animations,
}

/// One animation of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationRange {
    /// Whether the animation is also in the [`Section::Animations`] section.
    pub resident: bool,
    /// Where the animation is on its own, to be read when it is wanted.
    pub sectors: Sectors,
}

/// The record of one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRecord {
    /// Archive file of the model.
    pub file: u16,
    sections: [Sectors; 4],
    /// The model's animations by number.
    pub animations: Vec<AnimationRange>,
    /// Sets of several animations that are read in one go.
    pub sets: Vec<Sectors>,
    /// Further pieces with a mesh and textures of their own.
    pub parts: Vec<Sectors>,
}

impl ModelRecord {
    /// Reads the record at RAM address `at`.
    fn parse(image: &Image, at: u32) -> Result<Self, ModelError> {
        let word =
            |index: usize| -> Result<u16, ModelError> { Ok(image.u16(step(at, index, 2)?)?) };
        let pair = |index: usize| -> Result<Sectors, ModelError> {
            Ok(Sectors {
                start: word(index)?,
                count: word(index + 1)?,
            })
        };
        let file = word(0)?;
        let mut sections = [Sectors::default(); 4];
        for (index, section) in sections.iter_mut().enumerate() {
            *section = pair(1 + 2 * index)?;
        }
        // The marks come first, one word per animation, then the ranges.
        let count = word(9)? as usize;
        let animations = (0..count)
            .map(|index| {
                Ok(AnimationRange {
                    resident: word(10 + index)? != 0,
                    sectors: pair(10 + count + 2 * index)?,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        // Two lists follow, each a count and that many ranges.
        let mut next = 10 + 3 * count;
        let mut list = || -> Result<Vec<Sectors>, ModelError> {
            let count = word(next)? as usize;
            let ranges = (0..count).map(|index| pair(next + 1 + 2 * index)).collect();
            next += 1 + 2 * count;
            ranges
        };
        let sets = list()?;
        let parts = list()?;
        Ok(Self {
            file,
            sections,
            animations,
            sets,
            parts,
        })
    }

    /// Where a section is in the model file.
    pub fn section(&self, section: Section) -> Sectors {
        self.sections[section as usize]
    }

    /// Every range of the record in file order: sections, animations, sets, parts.
    pub fn ranges(&self) -> impl Iterator<Item = Sectors> + '_ {
        let animations = self.animations.iter().map(|animation| animation.sectors);
        (self.sections.iter().copied())
            .chain(animations)
            .chain(self.sets.iter().copied())
            .chain(self.parts.iter().copied())
    }

    /// The model's head section with its action table, read from the bytes of its file.
    pub fn head(&self, file: &[u8]) -> Result<Head, ModelError> {
        Head::parse(self.section(Section::Head).bytes(file)?).map_err(ModelError::Head)
    }

    /// The model's mesh block, read from the bytes of its file.
    pub fn mesh(&self, file: &[u8]) -> Result<MeshBlock, ModelError> {
        MeshBlock::parse(self.section(Section::Mesh).bytes(file)?).map_err(ModelError::Mesh)
    }

    /// The images of the model's texture set, read from the bytes of its file.
    pub fn textures(&self, file: &[u8]) -> Result<Vec<Tim>, ModelError> {
        tim::texture_set(self.section(Section::Textures).bytes(file)?).ok_or(ModelError::Textures)
    }

    /// Animation `number` of the model, read from its own sectors of the file.
    pub fn animation(&self, file: &[u8], number: usize) -> Result<Animation, ModelError> {
        let range = (self.animations.get(number)).ok_or(ModelError::NoAnimation(number))?;
        Animation::parse(range.sectors.bytes(file)?).map_err(ModelError::Animation)
    }

    /// Animation set `number` of the model: the animations it brings, in the order of the set.
    pub fn set(&self, file: &[u8], number: usize) -> Result<Vec<SetEntry>, ModelError> {
        let range = self.sets.get(number).ok_or(ModelError::NoSet(number))?;
        let entries = animation_set(range.bytes(file)?)?;
        match entries
            .iter()
            .find(|e| e.animation >= self.animations.len())
        {
            Some(_) => Err(ModelError::Set),
            None => Ok(entries),
        }
    }

    /// Part `number` of the model, read from its own sectors of the file.
    pub fn part(&self, file: &[u8], number: usize) -> Result<Part, ModelError> {
        let range = self.parts.get(number).ok_or(ModelError::NoPart(number))?;
        Part::parse(range.bytes(file)?)
    }
}

fn word(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..)?.get(..4)?;
    Some(u32::from_le_bytes(bytes.try_into().expect("four bytes")) as usize)
}

/// One animation of an animation set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetEntry {
    /// The number of the animation among the model's.
    pub animation: usize,
    pub block: Animation,
}

pub fn animation_set(data: &[u8]) -> Result<Vec<SetEntry>, ModelError> {
    let count = word(data, 0).ok_or(ModelError::Set)?;
    // The first block lies right behind the pairs.
    if count == 0 || count > data.len() / 8 || word(data, 8) != Some(4 + 8 * count) {
        return Err(ModelError::Set);
    }
    (0..count)
        .map(|index| {
            let animation = word(data, 4 + 8 * index).ok_or(ModelError::Set)?;
            let offset = word(data, 8 + 8 * index).ok_or(ModelError::Set)?;
            let block = data.get(offset..).ok_or(ModelError::Set)?;
            Ok(SetEntry {
                animation,
                block: Animation::parse(block).map_err(ModelError::Animation)?,
            })
        })
        .collect()
}

/// The word a part starts with.
const PART_MARK: usize = 3;
/// Where the pair list of a part is: right behind its four words.
const PART_HEAD: usize = 16;

/// A mesh object of the model and the source that takes its place while a part is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    pub object: i16,
    /// A number below the model's mesh objects is one of them, a number from there on a
    /// mesh object of the part.
    pub source: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub pairs: Vec<Pair>,
    /// The part's own mesh objects, numbered on from the model's.
    pub mesh: MeshBlock,
    /// Images that go where images of the model's own texture set lie.
    pub images: Vec<Tim>,
}

impl Part {
    /// Reads a part: the word 3, the offsets of its three pieces, the pieces.
    pub fn parse(data: &[u8]) -> Result<Self, ModelError> {
        let at = |index: usize| word(data, 4 * index).ok_or(ModelError::Part);
        let (mark, pairs_at, mesh_at, images_at) = (at(0)?, at(1)?, at(2)?, at(3)?);
        let count = word(data, PART_HEAD).ok_or(ModelError::Part)?;
        if mark != PART_MARK
            || pairs_at != PART_HEAD
            || count > data.len() / 4
            || mesh_at != PART_HEAD + 4 + 4 * count
            || images_at < mesh_at
        {
            return Err(ModelError::Part);
        }
        let half = |at: usize| -> Result<i16, ModelError> {
            let bytes = data.get(at..at + 2).ok_or(ModelError::Part)?;
            Ok(i16::from_le_bytes(bytes.try_into().expect("two bytes")))
        };
        let pairs = (0..count)
            .map(|index| {
                Ok(Pair {
                    object: half(PART_HEAD + 4 + 4 * index)?,
                    source: half(PART_HEAD + 6 + 4 * index)?,
                })
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        let mesh = data.get(mesh_at..images_at).ok_or(ModelError::Part)?;
        let mesh = MeshBlock::parse(mesh).map_err(ModelError::Mesh)?;
        let images = data.get(images_at..).ok_or(ModelError::Part)?;
        let images = tim::texture_set(images).ok_or(ModelError::Textures)?;
        Ok(Self {
            pairs,
            mesh,
            images,
        })
    }

    /// The model's mesh block with the part on: the part's mesh objects behind the model's,
    /// and every object of the pair list showing its source, as the swap routine leaves it.
    /// A pair that names an object or a source that is not there changes nothing.
    pub fn put_on(&self, model: &MeshBlock) -> MeshBlock {
        let mut block = model.clone();
        block.objects.extend(self.mesh.objects.iter().cloned());
        for pair in &self.pairs {
            let (Ok(object), Ok(source)) =
                (usize::try_from(pair.object), usize::try_from(pair.source))
            else {
                continue;
            };
            if object < model.objects.len() && source < block.objects.len() {
                block.objects[object] = block.objects[source].clone();
            }
        }
        block
    }

    pub fn kept_off(&self, model: &MeshBlock) -> MeshBlock {
        let mut block = model.clone();
        block.objects.extend(self.mesh.objects.iter().cloned());
        block
    }
}

/// File 27: the model numbers of the player's two models.
pub const PLAYER_MODELS: u32 = 0x800B_1334;
/// File 27: the part of the first model, by the first value.
pub const PARTS_BY_WEAPON: u32 = 0x800B_133C;
/// File 27: the share of the first value in the part of the second model.
pub const OFFSET_BY_WEAPON: u32 = 0x800B_1388;
/// Executable: the share of the second value in the part of the second model.
pub const BASE_BY_ARMOUR: u32 = 0x8007_D4AC;
/// Values the first of the player's two values takes.
pub const WEAPONS: usize = 19;
/// Entries of the executable's table that the 171 parts come from.
pub const ARMOURS: usize = 16;
/// Executable: the weapon records, whose 16-bit value at +2 is the first value.
pub const WEAPON_RECORDS: u32 = 0x8007_CF70;
/// Bytes of a weapon record.
pub const WEAPON_RECORD: usize = 24;
/// Weapon records the executable's getter hands out.
pub const WEAPON_ITEMS: usize = 23;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Worn {
    /// The weapon, a record of the executable's weapon table.
    pub weapon: u8,
    /// The armour, or `Worn::NOTHING`.
    pub armour: u8,
}

impl Worn {
    /// The byte of a place that holds nothing.
    pub const NOTHING: u8 = 0xFF;
    /// What the title overlay's setup of a game leaves: the first weapon and no armour.
    pub const AT_THE_START: Self = Self {
        weapon: 0,
        armour: Self::NOTHING,
    };

    /// The two of the three bytes the game carries that pick the model
    /// (`carried::Carried::worn`).
    pub fn of(bytes: crate::items::Worn) -> Self {
        Self {
            weapon: bytes[0],
            armour: bytes[1],
        }
    }
}

pub fn part_is_put_on(area: usize) -> bool {
    !matches!(area, 1 | 2)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartTables {
    /// The model without the second value (file 831) and the one with it (file 832).
    pub models: [usize; 2],
    pub by_weapon: [usize; WEAPONS],
    pub offset_by_weapon: [usize; WEAPONS],
    pub base_by_armour: [usize; ARMOURS],
    /// The first value of every weapon record.
    pub looks: [usize; WEAPON_ITEMS],
}

impl PartTables {
    /// Reads the tables from the field engine file as it is on the disc and from the loaded
    /// executable, whose first byte is at RAM address `base`.
    pub fn read(field_engine: &[u8], executable: &[u8], base: u32) -> Result<Self, ModelError> {
        let engine = Image {
            data: field_engine,
            base: FIELD_ENGINE_BASE,
        };
        let executable = Image {
            data: executable,
            base,
        };
        let mut tables = Self {
            models: [0; 2],
            by_weapon: [0; WEAPONS],
            offset_by_weapon: [0; WEAPONS],
            base_by_armour: [0; ARMOURS],
            looks: [0; WEAPON_ITEMS],
        };
        for (index, look) in tables.looks.iter_mut().enumerate() {
            *look = executable.u16(step(WEAPON_RECORDS, index, WEAPON_RECORD)? + 2)? as usize;
        }
        for (index, model) in tables.models.iter_mut().enumerate() {
            *model = engine.u32(step(PLAYER_MODELS, index, 4)?)? as usize;
        }
        for (index, part) in tables.by_weapon.iter_mut().enumerate() {
            *part = engine.u32(step(PARTS_BY_WEAPON, index, 4)?)? as usize;
        }
        for (index, offset) in tables.offset_by_weapon.iter_mut().enumerate() {
            *offset = engine.u8(step(OFFSET_BY_WEAPON, index, 1)?)? as usize;
        }
        for (index, part) in tables.base_by_armour.iter_mut().enumerate() {
            *part = executable.u8(step(BASE_BY_ARMOUR, index, 1)?)? as usize;
        }
        Ok(tables)
    }

    /// The part model number `model` wears: for the first of the player's models the entry
    /// of the weapon, for the second the sum of the weapon's and the armour's entries.
    /// `None` for another model or a value outside the tables.
    pub fn part_number(&self, model: usize, weapon: usize, armour: usize) -> Option<usize> {
        if model == self.models[0] {
            self.by_weapon.get(weapon).copied()
        } else if model == self.models[1] {
            Some(*self.offset_by_weapon.get(weapon)? + *self.base_by_armour.get(armour)?)
        } else {
            None
        }
    }

    /// The model and the part of a player who has this on, as the task that keeps the
    /// player's model up to date picks them: the second model with any armour, the first
    /// without, and the first value from the weapon's record. `None` for a weapon or an
    /// armour outside the tables.
    pub fn worn(&self, worn: Worn) -> Option<(usize, usize)> {
        let model = self.models[usize::from(worn.armour != Worn::NOTHING)];
        let look = *self.looks.get(usize::from(worn.weapon))?;
        Some((
            model,
            self.part_number(model, look, usize::from(worn.armour))?,
        ))
    }
}

/// The model table: a record per model, in the order of the game's model numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTable {
    records: Vec<ModelRecord>,
}

impl ModelTable {
    /// Reads the table from the loaded executable, whose first byte is at RAM address `base`.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, ModelError> {
        Self::parse(image, base, MODEL_TABLE)
    }

    /// Reads a table of record pointers at RAM address `table`, ended by a null pointer.
    pub fn parse(data: &[u8], base: u32, table: u32) -> Result<Self, ModelError> {
        let image = Image { data, base };
        let mut records = Vec::new();
        loop {
            let pointer = image.u32(step(table, records.len(), 4)?)?;
            if pointer == 0 {
                return Ok(Self { records });
            }
            if records.len() == MAX_MODELS {
                return Err(ModelError::NoEnd);
            }
            records.push(ModelRecord::parse(&image, pointer)?);
        }
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The record of model number `model`.
    pub fn get(&self, model: usize) -> Option<&ModelRecord> {
        self.records.get(model)
    }

    /// The model number and record of the model in archive file `file`.
    pub fn by_file(&self, file: u16) -> Option<(usize, &ModelRecord)> {
        self.iter().find(|(_, record)| record.file == file)
    }

    /// Every record with its model number.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &ModelRecord)> {
        self.records.iter().enumerate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{MeshObject, SeamVertex};

    const BASE: u32 = 0x8001_0000;

    /// An image with a table of two records at offset 0x100, the records at 0x10 and 0x80.
    fn image() -> Vec<u8> {
        let mut data = vec![0u8; 0x200];
        let mut put = |at: usize, words: &[u16]| {
            for (index, word) in words.iter().enumerate() {
                data[at + 2 * index..at + 2 * index + 2].copy_from_slice(&word.to_le_bytes());
            }
        };
        // File 900: sections, two animations (the second resident), one set, two parts.
        put(
            0x10,
            &[
                900, 0, 1, 1, 2, 3, 4, 7, 1, 2, 0, 1, 8, 3, 11, 2, 1, 13, 5, 2, 18, 1, 19, 1,
            ],
        );
        // File 901: no animations, no sets, no parts.
        put(0x80, &[901, 0, 2, 2, 1, 3, 1, 4, 1, 0, 0, 0]);
        data[0x100..0x104].copy_from_slice(&(BASE + 0x10).to_le_bytes());
        data[0x104..0x108].copy_from_slice(&(BASE + 0x80).to_le_bytes());
        data
    }

    fn sectors(start: u16, count: u16) -> Sectors {
        Sectors { start, count }
    }

    #[test]
    fn reads_records_to_the_null_pointer() {
        let table = ModelTable::parse(&image(), BASE, BASE + 0x100).unwrap();
        assert_eq!(table.len(), 2);
        let (model, record) = table.by_file(900).unwrap();
        assert_eq!(model, 0);
        assert_eq!(record.section(Section::Head), sectors(0, 1));
        assert_eq!(record.section(Section::Mesh), sectors(3, 4));
        assert_eq!(record.section(Section::Animations), sectors(7, 1));
        assert_eq!(
            record.animations,
            [
                AnimationRange {
                    resident: false,
                    sectors: sectors(8, 3)
                },
                AnimationRange {
                    resident: true,
                    sectors: sectors(11, 2)
                },
            ]
        );
        assert_eq!(record.sets, [sectors(13, 5)]);
        assert_eq!(record.parts, [sectors(18, 1), sectors(19, 1)]);
        // The ranges lie back to back.
        let mut end = 0;
        for range in record.ranges() {
            assert_eq!(range.start as usize, end);
            end = range.end();
        }
        assert_eq!(end, 20);

        let record = table.get(1).unwrap();
        assert_eq!(record.file, 901);
        assert!(record.animations.is_empty() && record.sets.is_empty() && record.parts.is_empty());
        assert!(table.get(2).is_none() && table.by_file(902).is_none());
    }

    #[test]
    fn a_pointer_outside_the_image_is_an_error() {
        let mut data = image();
        data[0x104..0x108].copy_from_slice(&0x8100_0000u32.to_le_bytes());
        assert_eq!(
            ModelTable::parse(&data, BASE, BASE + 0x100),
            Err(ModelError::OutOfImage(0x8100_0000))
        );
    }

    fn words(out: &mut Vec<u8>, values: &[u32]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    /// An animation block of `frames` frames for one bone: the counts, the rest offset and
    /// three entries per frame.
    fn block(frames: u32) -> Vec<u8> {
        let mut out = Vec::new();
        words(&mut out, &[frames, 1]);
        out.resize(8 + 6 + 18 * frames as usize, 0);
        out.resize(out.len().next_multiple_of(4), 0);
        out
    }

    #[test]
    fn reads_an_animation_set() {
        let (first, second) = (block(1), block(3));
        let mut data = Vec::new();
        words(&mut data, &[2, 7, 20, 4, 20 + first.len() as u32]);
        data.extend(&first);
        data.extend(&second);
        let entries = animation_set(&data).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].animation, entries[0].block.frames), (7, 1));
        assert_eq!((entries[1].animation, entries[1].block.frames), (4, 3));

        // The first block lies behind the pairs, and every block is in the set.
        let mut moved = data.clone();
        moved[8] = 24;
        assert_eq!(animation_set(&moved), Err(ModelError::Set));
        assert!(animation_set(&data[..data.len() - 4]).is_err());
        assert_eq!(animation_set(&[0; 8]), Err(ModelError::Set));
    }

    /// A mesh object that can be told from another by its one seam vertex.
    fn object(mark: i16) -> MeshObject {
        MeshObject {
            faces: Vec::new(),
            seam: vec![SeamVertex {
                position: [mark, 0, 0],
                normal: [0, 0, 4096],
                colour: [128; 3],
                uv: [0; 2],
                texture: None,
            }],
        }
    }

    /// A part with the pairs (0, 3) and (1, 1), a mesh block of two empty objects and an
    /// empty texture set.
    fn part() -> Vec<u8> {
        let mut data = Vec::new();
        words(&mut data, &[3, 16, 28, 28 + 96, 2]);
        for half in [0i16, 3, 1, 1] {
            data.extend_from_slice(&half.to_le_bytes());
        }
        words(&mut data, &[96, 2, 0, 0]);
        words(&mut data, &[0; 20]);
        words(&mut data, &[0]);
        data
    }

    #[test]
    fn reads_a_part_and_puts_it_on() {
        let mut part = Part::parse(&part()).unwrap();
        let pair = |object, source| Pair { object, source };
        assert_eq!(part.pairs, [pair(0, 3), pair(1, 1)]);
        assert_eq!(part.mesh.objects.len(), 2);
        assert!(part.images.is_empty());

        // A model of two objects: object 0 shows the part's second object, object 1 itself.
        part.mesh.objects = vec![object(10), object(11)];
        let model = MeshBlock {
            objects: vec![object(0), object(1)],
            ..Default::default()
        };
        let worn = part.put_on(&model);
        let marks: Vec<i16> = (worn.objects.iter())
            .map(|object| object.seam[0].position[0])
            .collect();
        assert_eq!(marks, [11, 1, 10, 11]);
        // A pair that leads nowhere changes nothing.
        part.pairs = vec![pair(5, 2), pair(0, 9), pair(-1, 0)];
        assert_eq!(part.put_on(&model).objects[..2], model.objects[..]);
    }

    #[test]
    fn a_part_must_start_as_one() {
        let good = part();
        for (at, value) in [(0, 2u8), (4, 20), (8, 32)] {
            let mut data = good.clone();
            data[at] = value;
            assert_eq!(Part::parse(&data), Err(ModelError::Part), "byte {at}");
        }
        assert!(Part::parse(&good[..good.len() - 8]).is_err());
    }

    #[test]
    fn the_part_tables_pick_a_part() {
        let tables = PartTables {
            models: [40, 39],
            by_weapon: std::array::from_fn(|weapon| 18 - weapon),
            offset_by_weapon: std::array::from_fn(|weapon| 9 * weapon),
            base_by_armour: std::array::from_fn(|armour| armour.saturating_sub(8)),
            looks: std::array::from_fn(|weapon| weapon % WEAPONS),
        };
        // No armour is the first model, whose part the weapon's look alone picks.
        assert_eq!(tables.worn(Worn::AT_THE_START), Some((40, 18)));
        let worn = |weapon, armour| tables.worn(Worn { weapon, armour });
        assert_eq!(worn(20, Worn::NOTHING), Some((40, 17)));
        assert_eq!(worn(20, 15), Some((39, 9 + 7)));
        assert_eq!(worn(23, 15), None);
        assert_eq!(worn(0, 16), None);
        assert_eq!([0, 1, 2, 3].map(part_is_put_on), [true, false, false, true]);
        assert_eq!(tables.part_number(40, 3, 15), Some(15));
        assert_eq!(tables.part_number(39, 3, 15), Some(27 + 7));
        assert_eq!(tables.part_number(39, 0, 0), Some(0));
        assert_eq!(tables.part_number(39, 19, 0), None);
        assert_eq!(tables.part_number(39, 0, 16), None);
        assert_eq!(tables.part_number(41, 0, 0), None);
    }

    #[test]
    fn reads_the_part_tables_from_the_two_images() {
        let at = |address: u32, base: u32| (address - base) as usize;
        let mut engine = vec![0u8; at(OFFSET_BY_WEAPON, FIELD_ENGINE_BASE) + WEAPONS];
        let mut put = |address: u32, value: u32| {
            let at = at(address, FIELD_ENGINE_BASE);
            engine[at..at + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(PLAYER_MODELS, 40);
        put(PLAYER_MODELS + 4, 39);
        put(PARTS_BY_WEAPON + 4 * 18, 5);
        engine[at(OFFSET_BY_WEAPON, FIELD_ENGINE_BASE) + 18] = 162;
        let mut executable = vec![0u8; at(BASE_BY_ARMOUR, BASE) + ARMOURS];
        executable[at(BASE_BY_ARMOUR, BASE) + 15] = 8;
        executable[at(WEAPON_RECORDS, BASE) + 22 * WEAPON_RECORD + 2] = 18;
        let tables = PartTables::read(&engine, &executable, BASE).unwrap();
        assert_eq!(tables.models, [40, 39]);
        assert_eq!(tables.looks[22], 18);
        let last = Worn {
            weapon: 22,
            armour: 15,
        };
        assert_eq!(tables.worn(last), Some((39, 170)));
        assert_eq!(tables.part_number(40, 18, 0), Some(5));
        assert_eq!(tables.part_number(39, 18, 15), Some(170));
        // An image that ends before a table is an error.
        assert_eq!(
            PartTables::read(&engine[..engine.len() - 1], &executable, BASE),
            Err(ModelError::OutOfImage(OFFSET_BY_WEAPON + 18))
        );
    }

    #[test]
    fn sectors_outside_the_file_are_an_error() {
        let file = vec![0u8; 3 * SECTOR];
        assert_eq!(sectors(1, 2).bytes(&file).unwrap().len(), 2 * SECTOR);
        assert_eq!(
            sectors(2, 2).bytes(&file),
            Err(ModelError::OutOfFile(sectors(2, 2)))
        );
    }
}
