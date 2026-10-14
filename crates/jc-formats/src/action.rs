
use std::fmt;

use crate::animation::Span;
use crate::skeleton::{BoneScale, GROUPS, ONE};

/// Bytes of a record of the point list.
const POINT_RECORD: usize = 12;
/// Bytes of an action of every model but the player's.
pub const SHORT_RECORD: usize = 60;
/// Bytes of an action of the player's models: four more, in front of the windows.
pub const LONG_RECORD: usize = 64;
/// The frame windows of an action.
pub const WINDOWS: usize = 10;
/// The 16-bit values behind the windows.
pub const TAIL: usize = 6;

/// Why a head section could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionError {
    /// The section ends before the piece at this offset does.
    Truncated(usize),
    /// The room of the table is not a whole number of records of a known size. Holds the room
    /// in bytes and the number of records.
    RecordSize(usize, usize),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(at) => write!(f, "the head section ends before offset {at:#x} does"),
            Self::RecordSize(room, count) => {
                write!(f, "{room} bytes are not {count} actions of a known size")
            }
        }
    }
}

impl std::error::Error for ActionError {}

/// One action of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    /// The animation of the model that the action plays; an unused entry has none.
    pub animation: Option<usize>,
    /// What the table says about where the animation is: 1 for one that stays in memory with
    /// the model, 0 for one that is read from the disc when the action is wanted.
    pub resident: u8,
    /// The two bytes only a long record has, zero in a short one.
    pub extra: [u8; 2],
    /// The frame the action starts on.
    pub first: i16,
    /// The frame the counter returns to after the end.
    pub loop_start: i16,
    /// A long record's second mark between loop start and end, where the player's step turns
    /// back while it holds the action. The end in a short record.
    pub hold: i16,
    /// The last frame.
    pub end: i16,
    /// Ten pairs of frames; -1 or -2 where there is none.
    pub windows: [[i16; 2]; WINDOWS],
    /// Six more values, -1 in most actions.
    pub tail: [i16; TAIL],
}

impl Action {
    /// Reads a record of [`SHORT_RECORD`] or [`LONG_RECORD`] bytes.
    fn parse(record: &[u8]) -> Self {
        let i16_at = |at: usize| i16::from_le_bytes([record[at], record[at + 1]]);
        let long = record.len() == LONG_RECORD;
        // A long record has two more bytes in front of the frames and a fourth frame.
        let frames = if long { 4 } else { 2 };
        let (first, loop_start) = (i16_at(frames), i16_at(frames + 2));
        let (hold, end, rest) = match long {
            true => (i16_at(frames + 4), i16_at(frames + 6), frames + 8),
            false => (i16_at(frames + 4), i16_at(frames + 4), frames + 6),
        };
        let mut windows = [[0; 2]; WINDOWS];
        for (index, window) in windows.iter_mut().enumerate() {
            *window = [i16_at(rest + 4 * index), i16_at(rest + 4 * index + 2)];
        }
        let mut tail = [0; TAIL];
        for (index, value) in tail.iter_mut().enumerate() {
            *value = i16_at(rest + 4 * WINDOWS + 2 * index);
        }
        Self {
            // The player's controller reads the byte of a long record unsigned: the
            // player's files have more than 127 animations and no unused entry.
            animation: match long {
                true => Some(usize::from(record[0])),
                false => usize::try_from(record[0] as i8).ok(),
            },
            resident: record[1],
            extra: if long { [record[2], record[3]] } else { [0; 2] },
            first,
            loop_start,
            hold,
            end,
            windows,
            tail,
        }
    }

    /// The frames an actor plays for the action.
    pub fn span(&self) -> Span {
        Span {
            first: self.first,
            loop_start: self.loop_start,
            end: self.end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub kind: i16,
    /// A bone by its place in the bone table; in a record that stands for another kind, that
    /// kind.
    pub bone: i16,
    /// The place in the bone's space, by all signs.
    pub place: [i16; 3],
    /// One more value, not read by anything found.
    pub last: i16,
}

impl Point {
    /// The kind this record only stands for, when it has no place of its own.
    pub fn stands_for(&self) -> Option<i16> {
        (self.place == [-1; 3] && self.last == -1).then_some(self.bone)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// The scale of the whole model, [`ONE`] being its stored size.
    pub scale: [i32; 3],
    /// The bone scales, a record per bone number.
    pub bones: Vec<BoneScale>,
    /// One value per bone group: the group is drawn when it is 2 or more.
    pub shown: [u16; GROUPS],
    /// One value per bone group: the group is drawn see-through when it is 0.
    pub marked: [u16; GROUPS],
    pub shares: Vec<i32>,
}

/// The share of the step to the next stage that the spawner adds to a stage's scale: a
/// fixed 4 of [`ONE`].
const GROWTH_SHARE: i32 = 4;

/// The scale of the whole model a creature of stage `stage` is spawned with: the stage's own
/// in the last stage, and below it the stage's plus a share of the step of the x value to the
/// next stage, added to all three. None when there is no such stage.
pub fn model_scale(stages: &[Stage], stage: usize) -> Option<[i32; 3]> {
    let scale = stages.get(stage)?.scale;
    Some(match stages.get(stage + 1) {
        Some(next) => scale.map(|value| value + (next.scale[0] - scale[0]) * GROWTH_SHARE / ONE),
        None => scale,
    })
}

/// RAM address in the executable of the levels at which a creature reaches its stages: a
/// byte per stage.
pub const STAGE_LEVELS: u32 = 0x8007_C208;
/// The stages the level table has.
const LEVELLED_STAGES: usize = 5;

pub fn stage_of_level(image: &[u8], base: u32, level: u8) -> Option<usize> {
    let offset = STAGE_LEVELS.checked_sub(base)? as usize;
    let levels = image.get(offset..)?.get(..LEVELLED_STAGES)?;
    let reached = levels.iter().take_while(|from| level >= **from).count();
    reached.checked_sub(1)
}

/// The points of a point list: more than this is not a list.
const MAX_POINTS: usize = 4096;
/// More stages, or bone scales in a stage, than this is not a list of stages.
const MAX_STAGES: usize = 64;
const MAX_STAGE_BONES: usize = 256;

/// The head section of a model file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    /// Bytes of an action in this file: [`SHORT_RECORD`] or [`LONG_RECORD`].
    pub record: usize,
    /// The model's actions by number.
    pub actions: Vec<Action>,
    /// The second and third piece of the block as offsets into `block`: the point list and the
    /// stages, which [`Self::points`] and [`Self::stages`] read.
    pub second: Option<usize>,
    pub third: Option<usize>,
    /// The block the game keeps: the section without its size word.
    pub block: Vec<u8>,
}

impl Head {
    /// Reads the section: a size, then a block of that size that starts with three offsets.
    pub fn parse(section: &[u8]) -> Result<Self, ActionError> {
        let u32_at = |data: &[u8], at: usize| -> Result<usize, ActionError> {
            let bytes = data.get(at..at + 4).ok_or(ActionError::Truncated(at))?;
            Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
        };
        let size = u32_at(section, 0)?;
        let block = (section.get(4..))
            .and_then(|rest| rest.get(..size))
            .ok_or(ActionError::Truncated(size))?;
        let table = u32_at(block, 0)?;
        let piece = |at: usize| -> Result<Option<usize>, ActionError> {
            Ok(Some(u32_at(block, at)?).filter(|offset| *offset != 0))
        };
        let (second, third) = (piece(4)?, piece(8)?);
        let mut head = Self {
            record: SHORT_RECORD,
            actions: Vec::new(),
            second,
            third,
            block: block.to_vec(),
        };
        // An offset of zero is a block without the piece.
        if table == 0 {
            return Ok(head);
        }
        let count = u32_at(block, table)?;
        let start = table + 4;
        // The size of a record is not stored: the game's readers know it. Here it is the room
        // up to the next piece of the block, shared out among the records.
        let end = [second, third, Some(block.len())]
            .into_iter()
            .flatten()
            .filter(|offset| *offset >= start)
            .min()
            .ok_or(ActionError::Truncated(start))?;
        if count == 0 {
            return Ok(head);
        }
        let room = end - start;
        head.record = [SHORT_RECORD, LONG_RECORD]
            .into_iter()
            .find(|record| record.checked_mul(count) == Some(room))
            .ok_or(ActionError::RecordSize(room, count))?;
        head.actions = block[start..end]
            .chunks_exact(head.record)
            .map(Action::parse)
            .collect();
        Ok(head)
    }

    fn i16_at(&self, at: usize) -> Result<i16, ActionError> {
        let bytes = (self.block.get(at..at + 2)).ok_or(ActionError::Truncated(at))?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u32_at(&self, at: usize) -> Result<usize, ActionError> {
        let bytes = (self.block.get(at..at + 4)).ok_or(ActionError::Truncated(at))?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
    }

    /// The point list, the second piece of the block: 12-byte records up to one that starts
    /// with -1. Empty when the block has no such piece.
    pub fn points(&self) -> Result<Vec<Point>, ActionError> {
        let Some(start) = self.second else {
            return Ok(Vec::new());
        };
        let mut points = Vec::new();
        for at in (start..).step_by(POINT_RECORD).take(MAX_POINTS) {
            let kind = self.i16_at(at)?;
            if kind == -1 {
                return Ok(points);
            }
            points.push(Point {
                kind,
                bone: self.i16_at(at + 2)?,
                place: [
                    self.i16_at(at + 4)?,
                    self.i16_at(at + 6)?,
                    self.i16_at(at + 8)?,
                ],
                last: self.i16_at(at + 10)?,
            });
        }
        Err(ActionError::Truncated(start))
    }

    /// The stages of a creature, the third piece of the block: two counts, then per stage
    /// five offsets and its length. Empty when the block has no such piece.
    pub fn stages(&self) -> Result<Vec<Stage>, ActionError> {
        let Some(start) = self.third else {
            return Ok(Vec::new());
        };
        let (count, bones) = (self.u32_at(start)?, self.u32_at(start + 4)?);
        if count > MAX_STAGES || bones > MAX_STAGE_BONES {
            return Err(ActionError::Truncated(start));
        }
        let mut at = start + 8;
        let mut stages = Vec::with_capacity(count);
        for _ in 0..count {
            let mut pieces = [0; 6];
            for (index, piece) in pieces.iter_mut().enumerate() {
                // Offsets from the stage's own start; the last is its length.
                *piece = (self.u32_at(at + 4 * index)?)
                    .checked_add(at)
                    .ok_or(ActionError::Truncated(at))?;
            }
            let [scale, scales, shown, marked, shares, end] = pieces;
            let mut stage = Stage {
                scale: [0; 3],
                bones: Vec::with_capacity(bones),
                shown: [0; GROUPS],
                marked: [0; GROUPS],
                shares: Vec::new(),
            };
            // The fifth table runs to the stage's end.
            for at in (shares..end).step_by(4) {
                stage.shares.push(self.u32_at(at)? as u32 as i32);
            }
            for (index, value) in stage.scale.iter_mut().enumerate() {
                *value = self.u32_at(scale + 4 * index)? as u32 as i32;
            }
            for bone in 0..bones {
                let record = scales + 8 * bone;
                stage.bones.push(BoneScale {
                    scale: [
                        self.i16_at(record)?,
                        self.i16_at(record + 2)?,
                        self.i16_at(record + 4)?,
                    ],
                    used: self.i16_at(record + 6)?,
                });
            }
            let groups = stage.shown.iter_mut().zip(&mut stage.marked);
            for (group, (is_shown, is_marked)) in groups.enumerate() {
                *is_shown = self.i16_at(shown + 2 * group)? as u16;
                *is_marked = self.i16_at(marked + 2 * group)? as u16;
            }
            if end <= at {
                return Err(ActionError::Truncated(at));
            }
            at = end;
            stages.push(stage);
        }
        Ok(stages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(values: &[i16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    /// A short record: animation, resident byte, three frames, ten windows, six values.
    fn short(animation: i8, frames: [i16; 3]) -> Vec<u8> {
        let mut record = vec![animation as u8, 1];
        record.extend(words(&frames));
        record.extend(words(&[7, 9]));
        record.extend(words(&[-2; 18]));
        record.extend(words(&[-1, -1, -1, -1, -1, 5]));
        assert_eq!(record.len(), SHORT_RECORD);
        record
    }

    /// A long record: two more bytes behind the resident byte and a fourth frame.
    fn long(animation: i8, extra: [u8; 2], frames: [i16; 4]) -> Vec<u8> {
        let mut record = vec![animation as u8, 0, extra[0], extra[1]];
        record.extend(words(&frames));
        record.extend(words(&[-1; 18]));
        record.extend(words(&[3, 4]));
        record.extend(words(&[-1; 6]));
        assert_eq!(record.len(), LONG_RECORD);
        record
    }

    /// A section: the size, three offsets, the table, and a second piece when wanted.
    fn section(records: &[Vec<u8>], second: bool) -> Vec<u8> {
        let table: Vec<u8> = records.concat();
        let mut block = Vec::new();
        block.extend(12u32.to_le_bytes());
        let behind = (12 + 4 + table.len()) as u32;
        block.extend((if second { behind } else { 0 }).to_le_bytes());
        block.extend(0u32.to_le_bytes());
        block.extend((records.len() as u32).to_le_bytes());
        block.extend(table);
        if second {
            block.extend([0xAA; 8]);
        }
        let mut section = (block.len() as u32).to_le_bytes().to_vec();
        section.extend(block);
        // A section is whole sectors: bytes behind the block are not part of it.
        section.extend([0xFF; 32]);
        section
    }

    #[test]
    fn short_records_are_read() {
        let head =
            Head::parse(&section(&[short(3, [0, 7, 23]), short(-1, [-1; 3])], true)).expect("head");
        assert_eq!(head.record, SHORT_RECORD);
        assert_eq!(head.second, Some(12 + 4 + 120));
        assert_eq!(head.third, None);
        assert_eq!(head.actions.len(), 2);
        let action = head.actions[0];
        assert_eq!(action.animation, Some(3));
        assert_eq!(action.resident, 1);
        assert_eq!(
            action.span(),
            Span {
                first: 0,
                loop_start: 7,
                end: 23
            }
        );
        assert_eq!(action.hold, 23);
        assert_eq!(action.windows[0], [7, 9]);
        assert_eq!(action.windows[9], [-2, -2]);
        assert_eq!(action.tail, [-1, -1, -1, -1, -1, 5]);
        assert_eq!(head.actions[1].animation, None);
    }

    #[test]
    fn long_records_are_told_by_the_room_of_the_table() {
        let records = [
            long(19, [2, 1], [0, 41, 62, 118]),
            long(0, [0, 3], [0, 20, 44, 44]),
            // The byte 0xC0: animation 192, not an unused entry.
            long(-64, [0, 0], [0, 0, 58, 59]),
        ];
        let head = Head::parse(&section(&records, false)).expect("head");
        assert_eq!(head.record, LONG_RECORD);
        let action = head.actions[0];
        assert_eq!(action.animation, Some(19));
        assert_eq!(action.extra, [2, 1]);
        assert_eq!((action.first, action.loop_start), (0, 41));
        assert_eq!((action.hold, action.end), (62, 118));
        assert_eq!(action.windows[8], [-1, -1]);
        assert_eq!(action.windows[9], [3, 4]);
        assert_eq!(head.actions[1].extra, [0, 3]);
        assert_eq!(head.actions[2].animation, Some(192));
    }

    #[test]
    fn a_table_of_another_record_size_is_an_error() {
        let mut odd = short(1, [0, 0, 5]);
        odd.extend([0; 2]);
        assert_eq!(
            Head::parse(&section(&[odd], false)),
            Err(ActionError::RecordSize(62, 1))
        );
    }

    /// A stage of two bone scale records in the layout of the disc's: five offsets and the
    /// length, the model's scale, the bone scales, the shown and the marked values, a last
    /// piece of four bytes.
    fn stage(scale: i32, bones: [[i16; 4]; 2], shown: [i16; 3], marked: [i16; 3]) -> Vec<u8> {
        let mut data = Vec::new();
        for offset in [24u32, 36, 52, 60, 68, 72] {
            data.extend(offset.to_le_bytes());
        }
        for _ in 0..3 {
            data.extend(scale.to_le_bytes());
        }
        data.extend(words(&bones.concat()));
        data.extend(words(&shown));
        data.extend(words(&[0]));
        data.extend(words(&marked));
        data.extend(words(&[0]));
        data.extend(2u32.to_le_bytes());
        assert_eq!(data.len(), 72);
        data
    }

    #[test]
    fn points_and_stages_are_read() {
        // A block without a table: the point list at 12, the stages behind it.
        let points = words(&[
            0, 0, 0, 0, 0, 0, //
            38, 7, 10, -20, 30, 5, //
            40, 38, -1, -1, -1, -1, //
            -1, 0, 0, 0, 0, 0,
        ]);
        let mut block = Vec::new();
        block.extend(0u32.to_le_bytes());
        block.extend(12u32.to_le_bytes());
        block.extend((12 + points.len() as u32).to_le_bytes());
        block.extend(points);
        block.extend(2u32.to_le_bytes());
        block.extend(2u32.to_le_bytes());
        let small = [[4096, 4096, 4096, 1], [3000, 3500, -1, 1]];
        block.extend(stage(2048, small, [0, 2, 3], [1, 0, 1]));
        block.extend(stage(
            4096,
            [[4096, 4096, 4096, 1]; 2],
            [1, 2, 3],
            [1, 1, 1],
        ));
        let mut whole = (block.len() as u32).to_le_bytes().to_vec();
        whole.extend(&block);

        let head = Head::parse(&whole).expect("head");
        assert!(head.actions.is_empty());
        let points = head.points().expect("points");
        assert_eq!(points.len(), 3);
        assert_eq!((points[1].kind, points[1].bone), (38, 7));
        assert_eq!((points[1].place, points[1].last), ([10, -20, 30], 5));
        assert_eq!(points[1].stands_for(), None);
        assert_eq!(points[2].stands_for(), Some(38));

        let stages = head.stages().expect("stages");
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].scale, [2048; 3]);
        assert_eq!(stages[0].bones.len(), 2);
        assert_eq!(stages[0].bones[1].scale, [3000, 3500, -1]);
        assert_eq!(stages[0].bones[1].used, 1);
        assert_eq!((stages[0].shown, stages[0].marked), ([0, 2, 3], [1, 0, 1]));
        assert_eq!(stages[1].shown, [1, 2, 3]);
        // The fifth table runs to the stage's end: one word here.
        assert_eq!(stages[0].shares, [2]);
        // Below the last stage the scale grows by 4 of 4096 of the step: 2048 * 4 / 4096.
        assert_eq!(model_scale(&stages, 0), Some([2050; 3]));
        assert_eq!(model_scale(&stages, 1), Some([4096; 3]));
        assert_eq!(model_scale(&stages, 2), None);

        // A block that ends inside a piece is an error, one without the pieces has none.
        let mut cut = ((block.len() - 80) as u32).to_le_bytes().to_vec();
        cut.extend(&block[..block.len() - 80]);
        let head = Head::parse(&cut).expect("head");
        assert!(matches!(head.stages(), Err(ActionError::Truncated(_))));
        let head = Head::parse(&section(&[short(1, [0, 0, 5])], false)).expect("head");
        assert_eq!(
            (head.points(), head.stages()),
            (Ok(Vec::new()), Ok(Vec::new()))
        );
    }

    #[test]
    fn a_level_gives_the_last_stage_it_has_reached() {
        // An image that starts eight bytes before the table.
        let mut image = vec![0u8; 8];
        image.extend([1, 7, 13, 22, 27]);
        let base = STAGE_LEVELS - 8;
        let stage = |level| stage_of_level(&image, base, level);
        assert_eq!(stage(0), None);
        assert_eq!((stage(1), stage(5), stage(6)), (Some(0), Some(0), Some(0)));
        assert_eq!(
            (stage(7), stage(12), stage(13)),
            (Some(1), Some(1), Some(2))
        );
        assert_eq!(
            (stage(21), stage(22), stage(26)),
            (Some(2), Some(3), Some(3))
        );
        assert_eq!((stage(27), stage(255)), (Some(4), Some(4)));
        // A table that is not whole in the image, or lies in front of it.
        assert_eq!(stage_of_level(&image[..12], base, 1), None);
        assert_eq!(stage_of_level(&image, STAGE_LEVELS + 4, 1), None);
    }

    #[test]
    fn a_short_section_is_an_error() {
        let whole = section(&[short(1, [0, 0, 5])], false);
        assert!(matches!(
            Head::parse(&whole[..40]),
            Err(ActionError::Truncated(_))
        ));
        assert!(matches!(Head::parse(&[]), Err(ActionError::Truncated(0))));
    }
}
