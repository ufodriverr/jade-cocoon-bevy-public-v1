
use crate::scene::ScenePackage;

/// The type of the object in a room's object list.
pub const KIND: u32 = 0x0A;
/// The bytes of a table entry: the movie and the music, a word each.
pub const ENTRY: usize = 8;
/// The game frames of the fade to black in front of the movie and of the fade behind
/// it (`0x8001AA50(30)`, `0x8001AA04(30)`).
pub const FADE: u16 = 30;

/// An entry of the object's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The movie's number in the executable's table of movies.
    pub movie: u32,
    /// The music that is asked for while the movie plays, none when negative.
    pub music: i32,
}

impl Entry {
    /// Entry `index` of the table at `table` (the data of the object's record). `None`
    /// when it does not lie in the package's code image.
    pub fn read(package: &ScenePackage, table: u32, index: u32) -> Option<Self> {
        let at = table.checked_add(index.checked_mul(ENTRY as u32)?)?;
        let bytes = package.bytes_at(at, ENTRY)?;
        let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().expect("4 bytes"));
        Some(Self {
            movie: word(0),
            music: word(4) as i32,
        })
    }
}

/// What a pass of the object's routine asks of the game, in the order it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
    /// The picture fades to black over that many game frames.
    FadeOut(u16),
    /// A music is asked for (`0x8003013C`); the routine has kept the number of the
    /// music that was noted last in front of it (`0x80031640`).
    Music(u32),
    /// The painted background is left out (`0x800A08FC`) and bit `0x08` of the engine's
    /// flag word is set.
    BackgroundOut,
    /// The movie player is called with this movie; the game does nothing else until
    /// it comes back.
    Movie(u32),
    /// The music that was kept is asked for again.
    MusicBack,
    /// The background is drawn again (`0x800A08DC`) and the bit is cleared.
    BackgroundIn,
    /// The picture comes back over that many game frames.
    FadeIn(u16),
}

/// The words of the object's task that its routine works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shower {
    /// The state a script has told and the routine has not taken yet (+124).
    told: Option<u32>,
    /// A telling is being worked (+20), the entry it named (+24) and the step (+32).
    running: bool,
    index: u32,
    step: u32,
    /// The state the routine reports (+128): 0 from the game frame the picture is
    /// black, 1 when the room is back.
    pub reached: i32,
}

impl Default for Shower {
    fn default() -> Self {
        Self::new()
    }
}

impl Shower {
    /// The task as the start routine leaves it.
    pub fn new() -> Self {
        Self {
            told: None,
            running: false,
            index: 0,
            step: 0,
            reached: 0,
        }
    }

    /// A script tells the object a state (`0x2E`): the number of a table entry.
    pub fn tell(&mut self, state: u32) {
        self.told = Some(state);
    }

    /// Whether a telling is being worked.
    pub fn runs(&self) -> bool {
        self.running
    }

    /// One game frame of the routine `0x800B2800`. `entry` reads an entry of the
    /// object's table, `at_rest` is whether the fade of the picture is at rest
    /// (`0x8001AB94`). A told state is taken only while no telling is worked, and
    /// its first step is made in the same pass.
    pub fn pass(&mut self, entry: impl Fn(u32) -> Option<Entry>, at_rest: bool) -> Vec<Asked> {
        if !self.running
            && let Some(told) = self.told.take()
        {
            (self.index, self.running) = (told, true);
        }
        if !self.running {
            return Vec::new();
        }
        let named = entry(self.index);
        let music = named.map_or(-1, |named| named.music);
        let mut asked = Vec::new();
        match self.step {
            0 => {
                asked.push(Asked::FadeOut(FADE));
                self.step += 1;
            }
            1 if at_rest => {
                if let Ok(music) = u32::try_from(music) {
                    asked.push(Asked::Music(music));
                }
                asked.push(Asked::BackgroundOut);
                self.reached = 0;
                self.step += 1;
            }
            4 => {
                asked.extend(named.map(|named| Asked::Movie(named.movie)));
                if music >= 0 {
                    asked.push(Asked::MusicBack);
                }
                self.step += 1;
            }
            7 => {
                asked.push(Asked::BackgroundIn);
                asked.push(Asked::FadeIn(FADE));
                self.step += 1;
            }
            8 if at_rest => {
                (self.running, self.step) = (false, 0);
                self.reached = 1;
            }
            1 | 8 => {}
            // The steps 2, 3, 5 and 6 only count on.
            _ => self.step += 1,
        }
        asked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(index: u32) -> Option<Entry> {
        [(3, 19), (2, -1)]
            .get(index as usize)
            .map(|&(movie, music)| Entry { movie, music })
    }

    #[test]
    fn a_told_entry_goes_black_plays_its_movie_and_comes_back() {
        let mut shower = Shower::new();
        assert_eq!(shower.pass(table, true), []);
        shower.tell(0);
        // The fade starts in the pass that takes the telling.
        assert_eq!(shower.pass(table, true), [Asked::FadeOut(FADE)]);
        assert!(shower.runs());
        // While the fade is not at rest nothing goes on.
        assert_eq!(shower.pass(table, false), []);
        assert_eq!(shower.pass(table, false), []);
        shower.reached = 1;
        assert_eq!(
            shower.pass(table, true),
            [Asked::Music(19), Asked::BackgroundOut]
        );
        assert_eq!(shower.reached, 0);
        // Two passes that only count, then the movie.
        assert_eq!(shower.pass(table, true), []);
        assert_eq!(shower.pass(table, true), []);
        assert_eq!(
            shower.pass(table, true),
            [Asked::Movie(3), Asked::MusicBack]
        );
        assert_eq!(shower.pass(table, true), []);
        assert_eq!(shower.pass(table, true), []);
        assert_eq!(
            shower.pass(table, true),
            [Asked::BackgroundIn, Asked::FadeIn(FADE)]
        );
        assert_eq!(shower.pass(table, false), []);
        assert_eq!((shower.reached, shower.runs()), (0, true));
        assert_eq!(shower.pass(table, true), []);
        assert_eq!((shower.reached, shower.runs()), (1, false));
        assert_eq!(shower.pass(table, true), []);
    }

    #[test]
    fn an_entry_without_a_music_asks_for_none_and_a_telling_waits_its_turn() {
        let mut shower = Shower::new();
        shower.tell(1);
        assert_eq!(shower.pass(table, true), [Asked::FadeOut(FADE)]);
        // A second telling is kept until the first is through.
        shower.tell(0);
        assert_eq!(shower.pass(table, true), [Asked::BackgroundOut]);
        shower.pass(table, true);
        shower.pass(table, true);
        assert_eq!(shower.pass(table, true), [Asked::Movie(2)]);
        for _ in 0..4 {
            shower.pass(table, true);
        }
        assert!(!shower.runs());
        assert_eq!(shower.pass(table, true), [Asked::FadeOut(FADE)]);
    }
}
