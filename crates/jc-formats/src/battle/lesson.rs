
use super::fight::Choice;

/// The bytes of a step.
pub const STEP: usize = 32;

/// Who a step is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    /// The player's fighter chooses, and the choice is judged.
    Player,
    /// Opponent 0 gets its command from the step in place of its own decision.
    Opponent,
    /// A message is shown.
    Message,
}

/// One step of a lesson.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub who: Who,
    /// The command: its kind, its parameter, the actor mask (`None` for "the acting
    /// slot") and the target mask.
    pub kind: i32,
    pub parameter: i32,
    pub actor: Option<u32>,
    pub targets: u32,
    /// The message of a message step; on the player's step the message of the refusal.
    pub message: i32,
    /// The player must choose the kind (`true`) or must not choose it.
    pub must: bool,
    /// The damage an attack, a spell or a body attack of this step does, in percent of
    /// the target's HP; 0 for none.
    pub percent: i32,
}

impl Step {
    /// A step from its eight words; `None` for the step that ends the list (a negative
    /// first word) and for a first word nobody is known by.
    pub fn parse(bytes: &[u8; STEP]) -> Option<Self> {
        let word = |n: usize| i32::from_le_bytes([0, 1, 2, 3].map(|byte| bytes[4 * n + byte]));
        let who = match word(0) {
            0 => Who::Player,
            4 => Who::Opponent,
            10 => Who::Message,
            _ => return None,
        };
        Some(Self {
            who,
            kind: word(1),
            parameter: word(2),
            actor: u32::try_from(word(3)).ok(),
            targets: word(4) as u32,
            message: word(5),
            must: word(6) == 1,
            percent: word(7),
        })
    }

    /// The judge of the player's choice (`0x800B44E8` of the gates' package): the
    /// message of the refusal when the kind he chose is not what the step asks, `None`
    /// when the command may run.
    pub fn refuses(&self, kind: i32) -> Option<i32> {
        ((kind == self.kind) != self.must).then_some(self.message)
    }

    /// The command of the step as a fighter is told it: what the opponent does on its
    /// step (`0x800B4478` writes it in place of the decision), and on the player's
    /// step the one choice the judge lets through. `None` for a message and for a
    /// kind no lesson of the disc has.
    pub fn choice(&self) -> Option<Choice> {
        let targets = self.targets as u8;
        let parameter = usize::try_from(self.parameter).ok()?;
        Some(match self.kind {
            0 => Choice::Attack { targets },
            1 => Choice::Spell {
                spell: parameter,
                targets,
            },
            4 => Choice::Item {
                item: parameter,
                targets,
            },
            5 => Choice::Capture { targets },
            6 => Choice::Body {
                slot: parameter,
                targets,
            },
            18 => Choice::Defend,
            _ => return None,
        })
    }

    /// The HP change the package's rewriter (`0x800B4560`) puts in place of the
    /// resolver's for the kinds 0, 1 and 6: the target's HP times the percent of the
    /// step / 100, plus the lowest bit of one draw. `None` for another kind.
    pub fn forced(&self, kind: i32, target_hp: i16, draw: u32) -> Option<i32> {
        let share = i32::from(target_hp) * self.percent / 100;
        matches!(kind, 0 | 1 | 6).then_some(share + (draw & 1) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(words: [i32; 8]) -> [u8; STEP] {
        let mut bytes = [0; STEP];
        for (place, word) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(words) {
            *place = word.to_le_bytes();
        }
        bytes
    }

    #[test]
    fn a_step_is_eight_words_and_a_negative_first_word_ends_the_list() {
        let attack = Step::parse(&step([0, 0, 0, 1, 16, 5, 1, 90])).expect("a step");
        let wanted = Step {
            who: Who::Player,
            kind: 0,
            parameter: 0,
            actor: Some(1),
            targets: 16,
            message: 5,
            must: true,
            percent: 90,
        };
        assert_eq!(attack, wanted);
        let says = Step::parse(&step([10, 24, 0, 0, 0, 11, 1, 0])).expect("a step");
        assert_eq!((says.who, says.message), (Who::Message, 11));
        let acting = Step::parse(&step([4, 0, 0, -1, 1, 0, 1, 36])).expect("a step");
        assert_eq!((acting.who, acting.actor), (Who::Opponent, None));
        assert_eq!(Step::parse(&step([-1, -1, 0, 0, 0, -1, 1, -1])), None);
        assert_eq!(Step::parse(&step([7, 0, 0, 0, 0, 0, 0, 0])), None);
    }

    #[test]
    fn the_judge_and_the_forced_amount() {
        let mut attack = Step::parse(&step([0, 0, 0, 1, 16, 5, 1, 90])).expect("a step");
        // The kind that is asked for runs; another one is refused with the message.
        assert_eq!((attack.refuses(0), attack.refuses(18)), (None, Some(5)));
        // 90 in 100 of 45 HP is 40, and the draw's lowest bit on top.
        assert_eq!(attack.forced(0, 45, 6), Some(40));
        assert_eq!(attack.forced(6, 45, 7), Some(41));
        assert_eq!(attack.forced(4, 45, 7), None);
        // A step that forbids its kind refuses exactly that one.
        attack.must = false;
        assert_eq!((attack.refuses(0), attack.refuses(18)), (Some(5), None));
    }
}
