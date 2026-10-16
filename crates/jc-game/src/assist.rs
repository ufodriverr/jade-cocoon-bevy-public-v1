
use std::sync::RwLock;

use bevy::prelude::*;
use jc_formats::battle::record::{LEVEL, Record};
use jc_formats::battle::results;
use jc_formats::items::{Keeping, MOST_MONEY};
use jc_formats::roaming::Stand;
use serde::{Deserialize, Serialize};

use crate::prefs::{self, Prefs};

/// The multipliers a setting may have; one outside is cut to them.
pub const LEAST: f64 = 0.5;
pub const MOST: f64 = 4.0;
/// The steps of a multiplier the experience is counted in: hundredths.
const STEPS: u128 = 100;

/// `respawn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Respawn {
    #[default]
    Original,
    Always,
}

/// `encounters`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Encounters {
    #[default]
    Normal,
    Fewer,
}

/// The assists as the game reads them: the multipliers inside their limits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Assists {
    pub exp: f64,
    pub money: f64,
    pub respawn: Respawn,
    pub encounters: Encounters,
}

impl Assists {
    /// The original game.
    pub const ORIGINAL: Self = Self {
        exp: 1.0,
        money: 1.0,
        respawn: Respawn::Original,
        encounters: Encounters::Normal,
    };

    pub fn of(assist: &prefs::Assist) -> Self {
        Self {
            exp: multiplier(assist.exp_multiplier),
            money: multiplier(assist.money_multiplier),
            respawn: assist.respawn,
            encounters: assist.encounters,
        }
    }
}

/// A multiplier of the settings inside its limits, in hundredths; one that is no number
/// is 1.
pub fn multiplier(value: f64) -> f64 {
    match value.is_finite() {
        true => (value.clamp(LEAST, MOST) * 100.0).round() / 100.0,
        false => 1.0,
    }
}

/// The assists of this run, for the parts of the game that are no systems (the room's
/// loading, its scripts).
static NOW: RwLock<Assists> = RwLock::new(Assists::ORIGINAL);

/// The assists of this run as the settings have them now.
pub fn now() -> Assists {
    NOW.read().map_or(Assists::ORIGINAL, |now| *now)
}

/// The assists of the settings are [`now`]'s from here on: at the start, before the
/// first room is read, and by [`reach`].
pub fn set(assist: &prefs::Assist) -> Assists {
    let assists = Assists::of(assist);
    if let Ok(mut now) = NOW.write() {
        *now = assists;
    }
    assists
}

/// The settings' assists reach [`now`] whenever they change (the options of a PC, a
/// loaded save's game feel).
pub fn reach(prefs: Res<Prefs>, mut had: Local<Option<Assists>>) {
    let assists = Assists::of(&prefs.feel.assist);
    if *had == Some(assists) {
        return;
    }
    *had = Some(set(&prefs.feel.assist));
    if assists != Assists::ORIGINAL {
        info!("the assists: {assists:?}");
    }
}

/// `--assist=exp:N,money:N,respawn:always,encounters:fewer`: the assists of one check.
pub fn for_a_check(assist: &mut prefs::Assist, args: &[String]) {
    let Some(value) = args.iter().find_map(|arg| arg.strip_prefix("--assist=")) else {
        return;
    };
    fn wrong(part: &str) -> ! {
        panic!(
            "--assist: {part:?} is not exp:N, money:N, respawn:original|always or \
             encounters:normal|fewer"
        )
    }
    for part in value.split(',') {
        let (name, value) = part.split_once(':').unwrap_or_else(|| wrong(part));
        let number = || value.trim().parse().unwrap_or_else(|_| wrong(part));
        match name.trim() {
            "exp" => assist.exp_multiplier = number(),
            "money" => assist.money_multiplier = number(),
            "respawn" => {
                assist.respawn = match value.trim() {
                    "original" => Respawn::Original,
                    "always" => Respawn::Always,
                    _ => wrong(part),
                }
            }
            "encounters" => {
                assist.encounters = match value.trim() {
                    "normal" => Encounters::Normal,
                    "fewer" => Encounters::Fewer,
                    _ => wrong(part),
                }
            }
            _ => wrong(part),
        }
    }
}

/// The sum of experience a fighter gets for `sum` (`results::sums`) with the experience
/// `times` as much: the worth the sum takes the fighter through, times `times`, from
/// where he stands (his record as the fight left it). What the multiplier adds never
/// takes him past level 99, and the sum always fits its word.
pub fn experience(fighter: &Record, sum: u32, times: f64) -> u32 {
    scaled_sum(fighter.level, fighter.experience, sum, times)
}

/// [`experience`] for a fighter of `level` with the experience word `way`.
fn scaled_sum(level: u8, way: u32, sum: u32, times: f64) -> u32 {
    let steps = (multiplier(times) * STEPS as f64).round() as u128;
    if sum == 0 || steps == STEPS {
        return sum;
    }
    let fighter = (level, way);
    let way = u64::from(fighter.1.min(LEVEL - 1));
    let had = (u64::from(fighter.0) << 26) + way;
    let after = had + u64::from(sum);
    let level_of = |word: u64| u32::try_from(word >> 26).unwrap_or(u32::MAX);
    let way_of = |word: u64| (word & u64::from(LEVEL - 1)) as u32;
    let before = results::worth(u32::from(fighter.0), way as u32);
    let gained = results::worth(level_of(after), way_of(after)).saturating_sub(before);
    let scaled = (gained / STEPS)
        .saturating_mul(steps)
        .saturating_add(gained % STEPS * steps / STEPS);
    let (levels, way) = results::levels_of(before.saturating_add(scaled));
    let word = (u64::from(levels) << 26) + u64::from(way.min(LEVEL - 1));
    let scaled = word.saturating_sub(had).min(u64::from(u32::MAX)) as u32;
    // A multiplier above 1 never gives less than the game, one below never more.
    match steps > STEPS {
        true => scaled.max(sum),
        false => scaled.min(sum),
    }
}

/// `amount` of money with the money `times` as much, rounded, at most the most money
/// there is; a positive amount stays at least 1.
pub fn money(amount: i32, times: f64) -> i32 {
    let times = multiplier(times);
    if amount <= 0 || times == 1.0 {
        return amount;
    }
    let scaled = (f64::from(amount) * times).round() as i64;
    scaled.clamp(1, i64::from(MOST_MONEY)) as i32
}

/// The player was given money since he had `before` (a script, a notice, a sale): he has
/// `times` as much of what he was given, and of a sale at most `cost`, what the things
/// cost to buy. Answers the money given, as the multiplier made it.
pub fn given_money(keeping: &mut Keeping, before: i32, times: f64, cost: Option<i32>) -> i32 {
    let given = keeping.money() - before;
    let mut scaled = money(given, times);
    if let Some(cost) = cost
        && scaled > given
    {
        scaled = scaled.min(cost.max(given));
    }
    match scaled.cmp(&given) {
        std::cmp::Ordering::Greater => {
            keeping.give_money(scaled - given);
        }
        std::cmp::Ordering::Less => {
            keeping.take_money(given - scaled);
        }
        std::cmp::Ordering::Equal => {}
    }
    keeping.money() - before
}

pub fn forgets(gone: bool, respawn: Respawn) -> bool {
    gone && respawn == Respawn::Original
}

/// The creatures a picking put into a room with `encounters = fewer`: the first half,
/// rounded up, and the others gone from the room's word too, so that a later visit has
/// the same. `word` is the room's word behind the picking, `filled` whether the picking
/// filled the room anew (its word had "fill the room anew"): a word that remembers its
/// creatures keeps them, so that the room does not lose one more on every visit. The
/// numbers of those that stay are their places in the answer as before.
pub fn thin(
    word: &mut u16,
    stands: Vec<Stand>,
    encounters: Encounters,
    filled: bool,
) -> Vec<Stand> {
    if encounters == Encounters::Normal || !filled {
        return stands;
    }
    let keep = stands.len().div_ceil(2);
    for gone in &stands[keep..] {
        *word &= !(31 << (5 * gone.place));
    }
    stands.into_iter().take(keep).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jc_formats::battle::fight::Spec;
    use jc_formats::roaming::{GROUPS, STANDS};

    /// What the tests need of a fighter's record: his level and his experience word.
    struct Fighter(u8, u32);

    fn record(level: u8, experience: u32) -> Fighter {
        Fighter(level, experience)
    }

    fn experience(fighter: &Fighter, sum: u32, times: f64) -> u32 {
        scaled_sum(fighter.0, fighter.1, sum, times)
    }

    #[test]
    fn a_multiplier_stays_inside_its_limits() {
        assert_eq!(multiplier(1.0), 1.0);
        assert_eq!(multiplier(0.1), 0.5);
        assert_eq!(multiplier(9.0), 4.0);
        assert_eq!(multiplier(f64::NAN), 1.0);
        assert_eq!(multiplier(1.234), 1.23);
        let assist = prefs::Assist {
            exp_multiplier: 7.0,
            ..default()
        };
        assert_eq!(Assists::of(&assist).exp, 4.0);
        assert_eq!(Assists::of(&prefs::Assist::default()), Assists::ORIGINAL);
    }

    #[test]
    fn the_experience_is_multiplied_as_worth_and_never_overflows() {
        let fighter = record(1, 0);
        // Once is the game's sum, bit for bit.
        assert_eq!(experience(&fighter, 0x0D00_0000, 1.0), 0x0D00_0000);
        assert_eq!(experience(&fighter, 0, 4.0), 0);
        // Half a level of a fighter of level 1: twice the worth is a whole level, as an
        // opponent of the own level is worth one.
        assert_eq!(experience(&fighter, LEVEL / 2, 2.0), LEVEL);
        // An opponent of the own level is one level; twice that worth from level 1 is
        // level 1 plus 1 and a half (the second level costs twice the first).
        assert_eq!(experience(&fighter, LEVEL, 2.0), LEVEL + LEVEL / 2);
        // Half of one level is half of the way to the next.
        assert_eq!(experience(&fighter, LEVEL, 0.5), LEVEL / 2);
        // The sum of the game, counted from a fighter of level 5 with a way, goes up.
        let grown = record(5, LEVEL / 4);
        let sum = results::gain(5, LEVEL / 4, 7);
        assert!(experience(&grown, sum, 2.0) > sum);
        assert!(experience(&grown, sum, 0.5) < sum);
        // A fighter near the top: never past level 99, never a word that overflows.
        let top = record(98, 0);
        let most = experience(&top, 3 * LEVEL, 4.0);
        assert!(most >= 3 * LEVEL);
        assert!(98 + (most >> 26) <= 99 + 2, "{most:#x}");
        let low = record(0, 0);
        let huge = experience(&low, 60 * LEVEL, 4.0);
        assert!(u64::from(huge) <= u64::from(u32::MAX));
        assert!(huge >= 60 * LEVEL);
    }

    #[test]
    fn money_is_multiplied_and_rounded_and_a_sale_brings_no_more_than_the_cost() {
        assert_eq!(money(100, 2.0), 200);
        assert_eq!(money(3, 0.5), 2);
        assert_eq!(money(1, 0.5), 1);
        assert_eq!(money(0, 4.0), 0);
        assert_eq!(money(MOST_MONEY, 4.0), MOST_MONEY);
        assert_eq!(money(77, 1.0), 77);
        let mut keeping = Keeping::new();
        keeping.give_money(50);
        // A gift of 30 with twice the money.
        keeping.give_money(30);
        assert_eq!(given_money(&mut keeping, 50, 2.0, None), 60);
        assert_eq!(keeping.money(), 110);
        // A sale of 75 (three quarters of 100) at four times is 100 at most.
        let before = keeping.money();
        keeping.give_money(75);
        assert_eq!(given_money(&mut keeping, before, 4.0, Some(100)), 100);
        // At half it is half.
        let before = keeping.money();
        keeping.give_money(75);
        assert_eq!(given_money(&mut keeping, before, 0.5, Some(100)), 38);
        // The most money is never passed.
        let mut rich = Keeping::new();
        rich.give_money(MOST_MONEY - 10);
        rich.give_money(5);
        assert_eq!(given_money(&mut rich, MOST_MONEY - 10, 4.0, None), 10);
        assert_eq!(rich.money(), MOST_MONEY);
    }

    #[test]
    fn always_keeps_a_beaten_creature_in_its_room() {
        assert!(forgets(true, Respawn::Original));
        assert!(!forgets(false, Respawn::Original));
        assert!(!forgets(true, Respawn::Always));
        assert!(!forgets(false, Respawn::Always));
    }

    /// The rule of the game behind it: a creature that is not forgotten is on its place
    /// again on the next visit, one that is leaves the room empty (and an emptied room
    /// gets nobody on a draw that divides by 3).
    #[test]
    fn a_creature_that_is_not_forgotten_is_back_on_the_next_visit() {
        use jc_formats::roaming::{FILL, Group, NOBODY, forget_in, pick_in};
        let row = [Group(16), Group(0), Group(0)];
        let list = [NOBODY; 4];
        for (respawn, back) in [(Respawn::Original, 0), (Respawn::Always, 1)] {
            let mut word = FILL;
            let first = pick_in(&mut word, &row, &list, 1, &mut || 1);
            assert_eq!(first.len(), 1);
            if forgets(true, respawn) {
                forget_in(&mut word, first[0].number);
            }
            let next = pick_in(&mut word, &row, &list, 1, &mut || 0);
            assert_eq!(next.len(), back, "{respawn:?}");
        }
    }

    fn stand(number: usize, place: usize) -> Stand {
        Stand {
            number,
            place,
            group: 0,
            specs: [Spec {
                species: 0,
                experience: 0,
                ai: 0,
            }; GROUPS],
        }
    }

    #[test]
    fn fewer_keeps_the_first_half_rounded_up_and_forgets_the_others() {
        let all = STANDS | STANDS << 5 | (1 | STANDS) << 10;
        let mut word = all;
        let three = vec![stand(0, 0), stand(1, 1), stand(2, 2)];
        let kept = thin(&mut word, three.clone(), Encounters::Fewer, true);
        assert_eq!(kept, three[..2]);
        assert_eq!(word, STANDS | STANDS << 5);
        // The next visit remembers the two and keeps them.
        let two = three[..2].to_vec();
        assert_eq!(thin(&mut word, two.clone(), Encounters::Fewer, false), two);
        assert_eq!(word, STANDS | STANDS << 5);
        // Normal changes nothing.
        let mut word = all;
        assert_eq!(
            thin(&mut word, three.clone(), Encounters::Normal, true),
            three
        );
        assert_eq!(word, all);
        // One stays one, two are one, none are none.
        let mut word = STANDS << 10;
        assert_eq!(
            thin(&mut word, vec![stand(0, 2)], Encounters::Fewer, true).len(),
            1
        );
        assert_eq!(word, STANDS << 10);
        let mut word = STANDS | STANDS << 10;
        let two = vec![stand(0, 0), stand(1, 2)];
        assert_eq!(thin(&mut word, two, Encounters::Fewer, true), [stand(0, 0)]);
        assert_eq!(word, STANDS);
        let mut word = 0;
        assert!(thin(&mut word, Vec::new(), Encounters::Fewer, true).is_empty());
    }

    #[test]
    fn a_check_sets_the_assists_by_its_option() {
        let mut assist = prefs::Assist::default();
        for_a_check(&mut assist, &["--smoke=1".into()]);
        assert_eq!(assist, prefs::Assist::default());
        let args = ["--assist=exp:2,money:1.5,respawn:always,encounters:fewer".to_string()];
        for_a_check(&mut assist, &args);
        assert_eq!(assist.exp_multiplier, 2.0);
        assert_eq!(assist.money_multiplier, 1.5);
        assert_eq!(assist.respawn, Respawn::Always);
        assert_eq!(assist.encounters, Encounters::Fewer);
        let text = toml::to_string(&assist).unwrap();
        assert!(text.contains("respawn = \"always\"") && text.contains("encounters = \"fewer\""));
    }
}
