
use crate::chapters::{VariantKey, VariantTable};
use crate::scene::ScenePackage;

/// The archive files of the scene packages that hold gates.
pub const PACKAGES: [usize; 5] = [299, 304, 305, 306, 307];
/// The type of the object, the extra bytes its task gets, and the bytes of its record.
const KIND: u32 = 0x0D;
const EXTRA: u32 = 0x10;
const SIZE: usize = 52;
/// The start routine calls the routine of the object here, and the routine names its
/// two scripts at these two places: an upper half, and a lower half one word behind.
const CALL: u32 = 0x24;
const SCRIPTS: [u32; 2] = [0xE8, 0xF8];
/// The extra bytes of an object for a player who comes from the place map, and where
/// its start routine names its script: an upper half, and a lower half one word behind.
const ARRIVAL_EXTRA: u32 = 4;
const ARRIVAL_SCRIPT: u32 = 0x30;
/// The area of the forests, whose places the gates lead to.
const FORESTS: i16 = 4;
/// The states a script tells a gate: show the opening, take the player across.
pub const SHOW: i32 = 1;
pub const CROSS: i32 = 2;

/// The record of a gate, the data of its object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    /// The thing of the story that opens the gate.
    pub thing: i32,
    /// The trigger region the player stands in to be at the gate, and the point the
    /// player then has to face.
    pub region: i32,
    pub point: [i32; 3],
    /// Where the gate puts the player, and the player's angle there.
    pub place: [i32; 3],
    pub angle: i16,
    /// The music the gate asks for when the player is across.
    pub music: i32,
    /// The two show lists of the package that the gate hands the spawner in turn.
    pub shows: [u32; 2],
    /// A place of the forests' area and the room its slot 0 gets when it has none yet;
    /// the place is negative for a gate that leads back.
    pub forest: i16,
    pub room: i16,
}

impl Gate {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; SIZE] = bytes.get(..SIZE)?.try_into().ok()?;
        let word = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        let half = |at: usize| i16::from_le_bytes(bytes[at..at + 2].try_into().expect("2 bytes"));
        Some(Self {
            thing: word(0),
            region: word(4),
            point: [word(8), word(12), word(16)],
            place: [word(20), word(24), word(28)],
            angle: half(32),
            music: word(36),
            shows: [word(40) as u32, word(44) as u32],
            forest: half(48),
            room: half(50),
        })
    }

    /// What the gate writes to the variant table when the player is across: the room of
    /// its forest's first slot, unless the slot has one.
    pub fn opens(&self, variants: &mut VariantTable) {
        let key = VariantKey {
            area: FORESTS,
            place: self.forest,
            slot: 0,
        };
        if self.forest >= 0 && variants.get(key).is_none() {
            variants.set(key, self.room);
        }
    }
}

/// The record of an object for a player who comes from the place map: the number story
/// variable 4 must hold, and the show list of the gate's opening on the side the player
/// comes out at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrival {
    pub number: i32,
    pub show: u32,
}

/// The gates of a package, each with the address of its record, and the two scripts
/// their routine starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gates {
    pub gates: Vec<(u32, Gate)>,
    /// The script for a player who has the gate's thing, and the one for a player who
    /// has not.
    pub with_thing: u32,
    pub without: u32,
    /// The objects for a player who comes from the place map, each with the address of
    /// its record, and the script the one with the right number starts.
    pub arrivals: Vec<(u32, Arrival)>,
    pub arrives: Option<u32>,
}

impl Gates {
    /// The gates of the package in archive file `file`; none for a package without.
    pub fn of(file: usize, package: &ScenePackage) -> Option<Self> {
        if !PACKAGES.contains(&file) {
            return None;
        }
        let word = |address: u32| {
            let bytes = package.bytes_at(address, 4)?;
            Some(u32::from_le_bytes(bytes.try_into().expect("4 bytes")))
        };
        let lists = package.object_lists();
        let mut gates: Vec<(u32, Gate)> = Vec::new();
        let mut start = None;
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == EXTRA)
        {
            if gates.iter().any(|(at, _)| *at == object.data) {
                continue;
            }
            gates.push((
                object.data,
                package.bytes_at(object.data, SIZE).and_then(Gate::parse)?,
            ));
            start = Some(object.function);
        }
        // The start routine's call of the routine: a `jal`.
        let call = word(start? + CALL)?;
        if call >> 26 != 3 {
            return None;
        }
        let routine = 0x8000_0000 | (call & 0x03FF_FFFF) << 2;
        let script = |at: u32| {
            let (upper, lower) = (word(routine + at)?, word(routine + at + 4)?);
            Some((upper << 16).wrapping_add(lower as u16 as i16 as u32))
        };
        // The objects for a player from the place map: a number and a show list each.
        // Their start routine hands the script's address over in `a0`.
        let mut arrivals: Vec<(u32, Arrival)> = Vec::new();
        let mut arrives = None;
        for object in (lists.iter().flat_map(|list| &list.objects))
            .filter(|object| object.kind == KIND && object.extra == ARRIVAL_EXTRA)
        {
            let (Some(number), Some(show)) = (word(object.data), word(object.data + 4)) else {
                continue;
            };
            if arrivals.iter().any(|(at, _)| *at == object.data)
                || !gates.iter().any(|(_, gate)| gate.shows.contains(&show))
            {
                continue;
            }
            let number = number as i32;
            arrivals.push((object.data, Arrival { number, show }));
            let at = object.function + ARRIVAL_SCRIPT;
            arrives = match (word(at), word(at + 4)) {
                (Some(upper), Some(lower)) if upper >> 16 == 0x3C04 && lower >> 16 == 0x2484 => {
                    Some((upper << 16).wrapping_add(lower as u16 as i16 as u32))
                }
                _ => arrives,
            };
        }
        Some(Self {
            gates,
            with_thing: script(SCRIPTS[0])?,
            without: script(SCRIPTS[1])?,
            arrivals,
            arrives,
        })
    }

    /// The gate on whose place the package's entry routine puts a player who comes with
    /// `number` in story variable 4 (1 to 8: the gates in their order), hidden.
    pub fn arrival_place(&self, number: i32) -> Option<&Gate> {
        let gate = usize::try_from(number).ok()?.checked_sub(1)?;
        self.gates.get(gate).map(|(_, gate)| gate)
    }

    /// The object that stays for a player who comes with `number` in story variable 4,
    /// with the address of its record; every other one ends in its start routine.
    pub fn arrival(&self, number: i32) -> Option<(u32, Arrival)> {
        (self.arrivals.iter().copied()).find(|(_, arrival)| arrival.number == number)
    }
}

/// What the object for a player from the place map did in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrived {
    Nothing,
    /// The show list at this address goes to the spawner.
    Shows(u32),
    /// Story variable 4 is cleared and the object ends.
    Ends,
    /// State 2 in the packages 304 and 307: this show list goes to the spawner and the
    /// task of the light is made; the object stays (`halo`).
    Lights(u32),
}

/// The words of the task of an object for a player from the place map: the state its
/// script told it (+124, -1 for none, -2 once it has ended) and the state it has reached
/// (+128).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arriving {
    pub told: i32,
    pub reached: i32,
}

impl Default for Arriving {
    fn default() -> Self {
        Self::new()
    }
}

impl Arriving {
    pub fn new() -> Self {
        Self {
            told: -1,
            reached: -1,
        }
    }

    /// The script tells the object a state (`0x30`).
    pub fn tell(&mut self, state: i32) {
        self.told = state;
    }

    /// One game frame of the task: told state 0 the show list goes to the spawner, told
    /// 1 or 2 the object clears story variable 4 and ends. In a package with the light
    /// of a lost fight (`halo`, its show list), told 2 starts the light and the object
    /// stays at reached state 2 until the script tells it 1.
    pub fn pass(&mut self, arrival: &Arrival, light: Option<u32>) -> Arrived {
        match (self.told, light) {
            (0, _) => {
                (self.reached, self.told) = (0, -1);
                Arrived::Shows(arrival.show)
            }
            (CROSS, Some(list)) => {
                (self.reached, self.told) = (CROSS, -1);
                Arrived::Lights(list)
            }
            (SHOW | CROSS, _) => {
                self.told = -2;
                Arrived::Ends
            }
            _ => Arrived::Nothing,
        }
    }
}

/// What a gate's routine is told about the game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Around {
    /// The player stands in the gate's region and faces its point.
    pub at: bool,
    /// The script the gate started last still runs.
    pub script_runs: bool,
    /// The player has the gate's thing of the story.
    pub has_thing: bool,
    /// The first record of the show list the gate handed over is still shown.
    pub shown: bool,
}

/// What a gate's routine did in a game frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passed {
    Nothing,
    /// A script starts: the one for a player with the gate's thing, or the other.
    Starts {
        with_thing: bool,
    },
    /// The show list at this address goes to the spawner.
    Shows(u32),
    /// The player is put on the gate's place.
    Crosses,
}

/// The words of a gate's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crossing {
    /// The state a script or a press told it (+124 of the task), -1 for none, and the
    /// state it has reached (+128).
    pub told: i32,
    pub reached: i32,
    /// The mark of the game frame (+120): a press of the confirm button tells the gate
    /// state 0.
    pub marked: bool,
    /// Which of the two show lists is next, the list that is shown, and whether the
    /// gate is being opened.
    pub side: usize,
    pub shown: Option<u32>,
    pub opening: bool,
}

impl Default for Crossing {
    fn default() -> Self {
        Self::new()
    }
}

impl Crossing {
    /// The task as the spawner and the start routine leave it.
    pub fn new() -> Self {
        Self {
            told: -1,
            reached: -1,
            marked: false,
            side: 0,
            shown: None,
            opening: false,
        }
    }

    /// A script tells the gate a state (`0x30` of a script the gate started).
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

    /// One game frame of the task: the wrapper that every object task of the package
    /// has, which takes the mark away, and the gate's routine. `over` is set when the
    /// shown list has gone in this game frame.
    pub fn pass(&mut self, gate: &Gate, around: Around) -> (Passed, bool) {
        self.marked = false;
        if matches!(self.told, -3 | -2) {
            return (Passed::Nothing, false);
        }
        if around.at && !self.opening && !around.script_runs {
            self.marked = true;
        }
        let over = self.shown.is_some() && !around.shown;
        if over {
            self.shown = None;
        }
        let passed = match self.told {
            0 => {
                self.opening = around.has_thing;
                self.told = -1;
                Passed::Starts {
                    with_thing: around.has_thing,
                }
            }
            SHOW => {
                let list = gate.shows[self.side];
                self.shown = Some(list);
                self.side ^= 1;
                (self.reached, self.told) = (SHOW, -1);
                Passed::Shows(list)
            }
            CROSS if self.shown.is_none() => {
                self.opening = false;
                (self.reached, self.told) = (CROSS, -1);
                Passed::Crosses
            }
            _ => Passed::Nothing,
        };
        (passed, over)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        let mut bytes = [0u8; SIZE];
        let words: [i32; 12] = [3, 1, 10, 20, 30, 40, 50, 60, -100, 7, 0x1000, 0x2000];
        for (n, word) in words.iter().enumerate() {
            bytes[4 * n..4 * n + 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes[48..50].copy_from_slice(&2i16.to_le_bytes());
        bytes[50..52].copy_from_slice(&118i16.to_le_bytes());
        Gate::parse(&bytes).expect("a record")
    }

    #[test]
    fn a_record_reads_its_fields() {
        let gate = gate();
        assert_eq!((gate.thing, gate.region), (3, 1));
        assert_eq!((gate.point, gate.place), ([10, 20, 30], [40, 50, 60]));
        assert_eq!((gate.angle, gate.music), (-100, 7));
        assert_eq!(gate.shows, [0x1000, 0x2000]);
        assert_eq!((gate.forest, gate.room), (2, 118));
        assert!(Gate::parse(&[0; SIZE - 1]).is_none());
    }

    #[test]
    fn a_press_at_the_gate_starts_a_script_and_the_script_takes_the_player_across() {
        let gate = gate();
        let mut crossing = Crossing::new();
        let at = Around {
            at: true,
            has_thing: true,
            ..Around::default()
        };
        // Away from the gate a press tells nothing.
        assert_eq!(crossing.pass(&gate, Around::default()).0, Passed::Nothing);
        assert!(!crossing.press());
        assert_eq!(crossing.pass(&gate, at).0, Passed::Nothing);
        assert!(crossing.press());
        let passed = crossing.pass(&gate, at).0;
        assert_eq!(passed, Passed::Starts { with_thing: true });
        // The gate is being opened: from the next game frame on there is no mark.
        let runs = Around {
            script_runs: true,
            ..at
        };
        crossing.tell(SHOW);
        assert_eq!(crossing.pass(&gate, runs).0, Passed::Shows(0x1000));
        assert!(!crossing.marked && !crossing.press());
        assert_eq!((crossing.reached, crossing.told), (SHOW, -1));
        // Told to take the player across, it waits for the shown list to go.
        crossing.tell(CROSS);
        let shown = Around {
            shown: true,
            ..runs
        };
        assert_eq!(crossing.pass(&gate, shown), (Passed::Nothing, false));
        assert_eq!(crossing.pass(&gate, runs), (Passed::Crosses, true));
        assert_eq!((crossing.reached, crossing.opening), (CROSS, false));
        // The next showing takes the other list.
        crossing.tell(SHOW);
        assert_eq!(crossing.pass(&gate, runs).0, Passed::Shows(0x2000));
        assert_eq!(crossing.side, 0);
    }

    #[test]
    fn without_the_thing_the_gate_stays_shut() {
        let gate = gate();
        let mut crossing = Crossing::new();
        let at = Around {
            at: true,
            ..Around::default()
        };
        crossing.pass(&gate, at);
        assert!(crossing.press());
        let passed = crossing.pass(&gate, at).0;
        assert_eq!(passed, Passed::Starts { with_thing: false });
        // It is not being opened: the mark of the game frame stands.
        assert!(crossing.marked);
    }

    #[test]
    fn a_player_from_the_place_map_comes_out_at_the_gate_of_the_number() {
        let (out, back) = (
            gate(),
            Gate {
                forest: -1,
                ..gate()
            },
        );
        let arrivals = [(1, 0x2000), (2, 0x1000)].map(|(number, show)| Arrival { number, show });
        let gates = Gates {
            gates: vec![(0x100, out), (0x134, back)],
            with_thing: 0x300,
            without: 0x400,
            arrivals: vec![(0x200, arrivals[0]), (0x208, arrivals[1])],
            arrives: Some(0x500),
        };
        // Number n is the place of gate n - 1; no number, no place and no object.
        assert_eq!(gates.arrival_place(2), Some(&back));
        assert_eq!(gates.arrival_place(0), None);
        assert_eq!(gates.arrival_place(3), None);
        assert_eq!(gates.arrival(2), Some((0x208, arrivals[1])));
        assert_eq!(gates.arrival(0), None);

        // The script tells the object to show, waits for it, and ends it at its end.
        let mut arriving = Arriving::new();
        assert_eq!(arriving.pass(&arrivals[0], None), Arrived::Nothing);
        arriving.tell(0);
        assert_eq!(arriving.pass(&arrivals[0], None), Arrived::Shows(0x2000));
        assert_eq!((arriving.reached, arriving.told), (0, -1));
        assert_eq!(arriving.pass(&arrivals[0], None), Arrived::Nothing);
        arriving.tell(SHOW);
        assert_eq!(arriving.pass(&arrivals[0], None), Arrived::Ends);
        assert_eq!(arriving.pass(&arrivals[0], None), Arrived::Nothing);

        // In a package with the light, state 2 starts it and the object stays at 2
        // until it is told 1.
        let mut arriving = Arriving::new();
        arriving.tell(CROSS);
        assert_eq!(
            arriving.pass(&arrivals[0], Some(0x3000)),
            Arrived::Lights(0x3000)
        );
        assert_eq!((arriving.reached, arriving.told), (CROSS, -1));
        assert_eq!(arriving.pass(&arrivals[0], Some(0x3000)), Arrived::Nothing);
        arriving.tell(SHOW);
        assert_eq!(arriving.pass(&arrivals[0], Some(0x3000)), Arrived::Ends);
    }

    #[test]
    fn a_gate_gives_its_forest_a_room_once() {
        let gate = gate();
        let mut variants = VariantTable::new();
        let key = VariantKey {
            area: FORESTS,
            place: 2,
            slot: 0,
        };
        gate.opens(&mut variants);
        assert_eq!(variants.get(key), Some(118));
        variants.set(key, 120);
        gate.opens(&mut variants);
        assert_eq!(variants.get(key), Some(120));
        // A gate that leads back writes nothing.
        let back = Gate { forest: -1, ..gate };
        let mut variants = VariantTable::new();
        back.opens(&mut variants);
        assert!(variants.entries().is_empty());
    }
}
