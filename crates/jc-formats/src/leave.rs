
use crate::scene::{Command, ScenePackage};

/// The told states the wrapper of an object task knows: the object rests, the task ends.
pub const RESTS: i32 = -3;
pub const ENDS: i32 = -2;

/// Whether the wrapper calls the routine of an object that was told this state.
pub fn runs(told: i32) -> bool {
    !matches!(told, RESTS | ENDS)
}

/// The type of the object and the bytes of its record.
pub const KIND: u32 = 0x00;
const SIZE: usize = 4;
/// The call that starts a script (`jal 0x800A5960`), and how far behind the routine's
/// start it is looked for.
const START_SCRIPT: u32 = 0x0C02_9658;
const REACH: u32 = 0x30;
/// The commands of the script: the argument, the routine, the place map, the end.
const SCRIPT: [u32; 4] = [0x4B, 0x4D, 0x58, 0x03];

/// The script a routine starts: the address its words build in `a0` around the call
/// that starts a script, an upper half in front of the call and a lower half behind it.
pub fn started_script(words: &[u32]) -> Option<u32> {
    let call = words.iter().position(|word| *word == START_SCRIPT)?;
    let (upper, lower) = (*words.get(call.checked_sub(1)?)?, *words.get(call + 1)?);
    // `lui a0` and `addiu a0,a0`.
    if upper >> 16 != 0x3C04 || lower >> 16 != 0x2484 {
        return None;
    }
    Some((upper << 16).wrapping_add(lower as u16 as i16 as u32))
}

/// A trigger of the place map: the record of an object of type `0x00` and what its
/// routine starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapTrigger {
    /// RAM address of the record, by which a script names the trigger (`0x2E`).
    pub record: u32,
    /// The views of the room in which the trigger is active.
    pub view_mask: u32,
    /// The trigger region of the room the player has to stand in.
    pub region: i16,
    /// The routine of the script makes the player stand again (byte 2 of the record).
    pub stands_again: bool,
    /// The script the routine starts, and the routine of the package that script calls.
    pub script: u32,
    pub routine: u32,
}

impl MapTrigger {
    /// The region and the byte of a record.
    pub fn parse(bytes: &[u8]) -> Option<(i16, bool)> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        Some((i16::from_le_bytes([bytes[0], bytes[1]]), bytes[2] != 0))
    }

    /// The triggers of the place map in the object lists of a package, each once.
    pub fn of(package: &ScenePackage) -> Vec<Self> {
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let mut triggers: Vec<Self> = Vec::new();
        let lists = package.object_lists();
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.function != 0)
        {
            if triggers.iter().any(|known| known.record == object.data) {
                continue;
            }
            let words: Vec<u32> = (0..REACH)
                .map_while(|n| word(object.function + 4 * n))
                .collect();
            let read = || {
                let (region, stands_again) =
                    package.bytes_at(object.data, SIZE).and_then(Self::parse)?;
                let script = started_script(&words)?;
                let commands: Vec<Command> = package.commands_at(script);
                let codes: Vec<u32> = commands.iter().map(|command| command.code).collect();
                (codes == SCRIPT).then(|| Self {
                    record: object.data,
                    view_mask: object.view_mask,
                    region,
                    stands_again,
                    script,
                    routine: commands[1].c,
                })
            };
            triggers.extend(read());
        }
        triggers
    }
}

/// The words of a trigger's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Waiting {
    /// The state a script told it (+124 of the task), -1 for none.
    pub told: i32,
    /// The routine has started its script (+140 holds the record from then on).
    pub started: bool,
}

impl Default for Waiting {
    fn default() -> Self {
        Self::new()
    }
}

impl Waiting {
    /// The task as the spawner leaves it.
    pub fn new() -> Self {
        Self {
            told: -1,
            started: false,
        }
    }

    /// A script tells the trigger a state (`0x2E`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the task. `inside`: the player stands in the trigger's region.
    /// True in the game frame the script starts.
    pub fn pass(&mut self, inside: bool) -> bool {
        if !runs(self.told) || !inside || self.started {
            return false;
        }
        self.started = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_told_state_of_minus_three_or_minus_two_keeps_the_routine_from_running() {
        assert!(runs(-1) && runs(0) && runs(4));
        assert!(!runs(RESTS) && !runs(ENDS));
    }

    #[test]
    fn a_record_reads_its_region_and_its_byte() {
        assert_eq!(MapTrigger::parse(&[2, 0, 1, 1]), Some((2, true)));
        assert_eq!(MapTrigger::parse(&[0xFF, 0xFF, 0, 1]), Some((-1, false)));
        assert_eq!(MapTrigger::parse(&[0; 3]), None);
    }

    #[test]
    fn the_script_is_the_address_around_the_call() {
        // `lui a0,0x800B`, the call, `addiu a0,a0,0x3FC4`.
        let words = [0x27BD_FFE0, 0x3C04_800B, START_SCRIPT, 0x2484_3FC4];
        assert_eq!(started_script(&words), Some(0x800B_3FC4));
        // A lower half with its top bit set takes one off the upper half.
        let words = [0x3C04_800C, START_SCRIPT, 0x2484_8000];
        assert_eq!(started_script(&words), Some(0x800B_8000));
        // No call, or other registers around it.
        assert_eq!(started_script(&[0x3C04_800B, 0x2484_3FC4]), None);
        assert_eq!(
            started_script(&[0x3C05_800B, START_SCRIPT, 0x2484_3FC4]),
            None
        );
        assert_eq!(started_script(&[START_SCRIPT, 0x2484_3FC4]), None);
    }

    #[test]
    fn a_trigger_starts_its_script_once_and_not_while_it_rests() {
        let mut waiting = Waiting::new();
        assert!(!waiting.pass(false));
        waiting.tell(RESTS);
        assert!(!waiting.pass(true));
        waiting.tell(-1);
        assert!(waiting.pass(true));
        assert!(!waiting.pass(true));
    }
}
