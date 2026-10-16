
use super::record::{Draws, Record, Side, number, status};
use super::{Descriptor, Move, Tables, class};

/// What the fighter was told to do, as far as the rules of a hit know commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Kind 0: the weapon for the player, the plain attack for everybody else.
    Attack,
    /// Kind 1, with the number of the spell.
    Spell(usize),
    /// Kind 4, with the number of the item.
    Item(usize),
    /// Kind 6, with the attack slot of the actor's move entries.
    Body(usize),
    /// Kinds 5 and 26.
    Capture,
    /// Kind 7.
    Flee,
    /// Any other kind: nothing is decided here.
    Other,
}

/// The outcome of a result: what the screen shows for the target.
pub mod outcome {
    /// Nothing to show; also the flight.
    pub const NOTHING: u16 = 0;
    pub const HIT: u16 = 1;
    pub const MP_HIT: u16 = 2;
    pub const HP_AND_MP: u16 = 3;
    /// A critical hit, or a "down to 1 HP" that worked.
    pub const CRITICAL: u16 = 4;
    /// A miss, or no effect.
    pub const MISS: u16 = 5;
    pub const CAPTURE: u16 = 7;
}

/// The bits of the target's byte in the effect block.
pub mod effect {
    /// The action missed or did nothing.
    pub const MISSED: u8 = 0x06;
    /// A status was refused: the roll failed, or a shield took it.
    pub const REFUSED: u8 = 0x04;
    /// Which layer of the shield went: the first, the second, the last.
    pub const FIRST_LAYER: u8 = 0x08;
    pub const SECOND_LAYER: u8 = 0x10;
    pub const LAST_LAYER: u8 = 0x20;
}

/// The statuses a shield keeps away, and its three layers.
const BAD: u32 = 0x07FB;
const SHIELD: u32 = 0x3800;
/// The most a grade adds to a body attack's power or accuracy, and to its critical byte.
const HIGHEST_BYTE: i32 = 255;
const HIGHEST_CRITICAL: i32 = 125;
/// No deva on the field, and the lock that lets it change.
pub const NO_DEVA: i16 = -1;

/// One call of the resolver: a command, who acts and one target.
#[derive(Debug, Clone, Copy)]
pub struct Order<'a> {
    pub command: Command,
    /// The records themselves, not the copies with the equipment.
    pub actor: &'a Record,
    pub actor_side: Side,
    pub target: &'a Record,
    pub target_side: Side,
    /// The deva on the field ([`NO_DEVA`] for none), and [`NO_DEVA`] when the action
    /// may change it.
    pub deva: i16,
    pub lock: i16,
    /// The lowest bit of the frame counter when the command is resolved.
    pub odd_frame: bool,
}

/// The result record of the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resolved {
    /// What to show (`outcome`).
    pub outcome: u16,
    /// The grade of a capture (0 and 2 catch), or for a flight 0 got away, 1 stopped.
    pub grade: u16,
    /// Status bits to clear on the target, then bits to add to it.
    pub clear: u32,
    pub add: u32,
    /// The MP the action costs the actor.
    pub cost: i32,
    /// The change of the actor's HP and MP; negative is a gain (drain).
    pub actor_hp: i32,
    pub actor_mp: i32,
    /// The change of the target's HP and MP; positive is a loss.
    pub hp: i32,
    pub mp: i32,
    /// The deva on the field after this action.
    pub deva: i16,
    /// The target's byte in the effect block (`effect`).
    pub effects: u8,
}

/// The product of two doubles as the game's own routine gives it (`0x8005D19C`): to the
/// nearest, but a product that lies exactly between two doubles goes away from zero.
pub fn times(a: f64, b: f64) -> f64 {
    let product = a * b;
    if product == 0.0 || !product.is_finite() {
        return product;
    }
    // What the rounding cut off, exactly.
    let rest = product_rest(a, b, product);
    let away = if product > 0.0 {
        product.next_up()
    } else {
        product.next_down()
    };
    if rest != 0.0 && 2.0 * rest == away - product {
        away
    } else {
        product
    }
}

fn product_rest(a: f64, b: f64, product: f64) -> f64 {
    const HUGE: f64 = 1e150;
    if a.abs() > HUGE || b.abs() > HUGE {
        return a.mul_add(b, -product);
    }
    // 2^27 + 1 cuts a double into two halves of 26 bits each.
    const SPLIT: f64 = 134_217_729.0;
    let halves = |x: f64| {
        let c = SPLIT * x;
        let high = c - (c - x);
        (high, x - high)
    };
    let ((ah, al), (bh, bl)) = (halves(a), halves(b));
    al * bl - (((product - ah * bh) - al * bh) - ah * bl)
}

/// The game's square root (`0x8005E52C`): Newton's steps from the larger of the number
/// and 1, to the first step that does not go down. Not always the nearest double.
pub fn root(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut y = x.max(1.0);
    loop {
        let next = times(x / y + y, 0.5);
        if next < y {
            y = next;
        } else {
            return y;
        }
    }
}

/// The descriptor of a body attack as the rules use it (`0x800129B0`): changed by the
/// target's body flags, when it has any, and by the grade of the move entry.
fn body_attack(mut d: Descriptor, entry: &Move, target: Option<u8>) -> Descriptor {
    let raised = |byte: i32, by: i32, highest: i32| (byte + by).min(highest);
    if let Some(flags) = target {
        match entry.attack {
            15 if flags & 2 != 0 => {
                d.critical = raised(d.critical.into(), 60, HIGHEST_CRITICAL) as i8;
            }
            14 if flags & 1 != 0 => {
                d.critical = raised(d.critical.into(), 60, HIGHEST_CRITICAL) as i8;
            }
            12 if flags & 2 != 0 => d.power = 0,
            _ => {}
        }
    }
    match entry.grade {
        2 => d.power = raised(d.power.into(), 30, HIGHEST_BYTE) as i16,
        3 => {
            d.accuracy = raised(d.accuracy.into(), 30, HIGHEST_BYTE) as u8;
            d.rate = raised(d.rate.into(), 20, HIGHEST_BYTE) as u8;
        }
        4 => d.critical = raised(d.critical.into(), 50, HIGHEST_CRITICAL) as i8,
        _ => {}
    }
    d
}

/// Whether the descriptor of `order`'s command is a drain (`0x80013228`: its class is
/// 8), which the menu's preview asks before it counts the actor's own share.
pub fn drains(tables: &Tables, order: &Order) -> bool {
    let descriptor = match order.command {
        Command::Attack if order.actor_side == Side::Player => {
            let weapon = tables.weapons.get(usize::from(order.actor.equipment[0]));
            weapon.and_then(|weapon| weapon.effect)
        }
        Command::Spell(spell) => tables.spells.get(spell).map(|spell| spell.effect),
        Command::Item(item) => tables.items.get(item).map(|item| item.effect),
        Command::Body(slot) => (order.actor.moves.get(slot))
            .and_then(|entry| tables.body_attacks.get(usize::from(entry.attack)).copied()),
        _ => Some(tables.plain_attack),
    };
    descriptor.is_some_and(|descriptor| descriptor.class == class::DRAIN)
}

/// Decides `order`. `None` when the command names a row the tables do not have (a
/// spell, an item, a body attack, or the weapon of a player without one: what the game
/// does then is not read).
pub fn resolve(tables: &Tables, order: &Order, draws: &mut impl Draws) -> Option<Resolved> {
    let Order { actor, target, .. } = *order;
    let flags = |record: &Record| tables.body_flags.get(record.species as usize).copied();
    let mut result = Resolved {
        deva: order.deva,
        ..Resolved::default()
    };
    let mut d = match order.command {
        Command::Attack if order.actor_side == Side::Player => {
            tables
                .weapons
                .get(usize::from(actor.equipment[0]))?
                .effect?
        }
        Command::Spell(spell) => {
            let spell = tables.spells.get(spell)?;
            result.cost = spell.cost.into();
            spell.effect
        }
        Command::Item(item) => tables.items.get(item)?.effect,
        Command::Body(slot) => {
            let entry = actor.moves.get(slot)?;
            result.cost = entry.cost();
            let attack = *tables.body_attacks.get(usize::from(entry.attack))?;
            body_attack(attack, entry, flags(target))
        }
        _ => tables.plain_attack,
    };
    let (acting, hit) = (actor.effective(tables), target.effective(tables));
    let na = acting.numbers(d.accuracy, order.actor_side);
    let nt = hit.numbers(d.accuracy, order.target_side);

    // The deva on the field helps its own element, and the action may call another.
    if order.deva == i16::from(d.class) && d.power != 0 {
        d.power += 60;
    }
    let called = i16::from(d.deva);
    if order.lock == NO_DEVA && called != NO_DEVA {
        let beats = matches!((called, result.deva), (0, 1) | (3, 0) | (2, 3) | (1, 2));
        if beats || result.deva == NO_DEVA {
            result.deva = called;
        }
    }
    // The body bonus, by the actor's own flags.
    if let Some(flags) = flags(actor)
        && d.power > 0
    {
        let helped = match order.command {
            Command::Spell(_) => flags & 2 != 0,
            Command::Body(_) => flags & 1 != 0,
            _ => false,
        };
        if helped {
            d.power += 10;
        }
    }

    let sleeps = hit.status & status::SLEEP != 0;
    match order.command {
        Command::Attack | Command::Spell(_) | Command::Item(_) | Command::Body(_) => {
            decide(
                &d,
                &acting,
                &hit,
                &na,
                &nt,
                order.odd_frame,
                &mut result,
                draws,
            );
        }
        Command::Capture => {
            result.outcome = outcome::CAPTURE;
            // The records themselves: the player's own level counts, not his weapon's.
            // A target without HP is not guarded: the console's division answers -1.
            let mut score = match i32::from(hit.hp) {
                0 => -1,
                hp => i32::from(hit.max_hp) / hp,
            };
            let ahead = (i32::from(actor.level) - i32::from(target.level.min(40))) * 10 / 12;
            if ahead > 0 {
                score = score.wrapping_shl(ahead.min(5) as u32);
            } else if ahead < 0 {
                score /= 1 << (-ahead).min(5);
            }
            if sleeps {
                score = score.wrapping_shl(3);
            }
            let tier = (tables.capture_steps.iter())
                .take_while(|step| i32::from(**step) < score)
                .count();
            let drawn = draws.draw() % 16;
            let row = tables.capture.get(tier)?;
            let grade = row.iter().take_while(|below| drawn >= u32::from(**below));
            result.grade = grade.count() as u16;
            result.clear |= status::SLEEP;
        }
        Command::Flee => {
            let drawn = draws.draw() % 100;
            result.grade = u16::from(drawn >= 81 && !sleeps);
        }
        Command::Other => {}
    }
    if result.outcome == outcome::MISS {
        result.effects |= effect::MISSED;
    }
    Some(result)
}

/// The part of an attack, a spell, an item or a body attack (`0x80013374`): the hit,
/// the amount, the status and the shield.
#[allow(clippy::too_many_arguments)]
fn decide(
    d: &Descriptor,
    acting: &Record,
    hit: &Record,
    na: &[i32; 10],
    nt: &[i32; 10],
    odd_frame: bool,
    result: &mut Resolved,
    draws: &mut impl Draws,
) {
    let power = i32::from(d.power);
    let (mut attack, mut defence, mut accuracy, mut evasion, mut critical, mut anti) =
        (0, 0, 0, 0, 0, 0);
    match d.pair {
        0 => {
            (attack, accuracy, critical) = (
                na[number::ATTACK],
                na[number::ACCURACY],
                na[number::CRITICAL],
            );
            (defence, evasion, anti) = (nt[number::DEFENCE], nt[number::EVASION], nt[7]);
        }
        1 => {
            (attack, defence, accuracy) =
                (na[number::MAGIC_ATTACK], nt[number::MAGIC_DEFENCE], 100);
        }
        2 => (attack, defence, accuracy) = (power, 1, 100),
        _ => {}
    }
    // The target's modifier for the class; the classes without one count as 0.
    let modifier = (hit.modifiers.get(usize::from(d.class))).map_or(0, |m| i32::from(*m));
    if modifier < 0 {
        (defence, evasion) = (1, 1);
    }
    if defence == 0 {
        defence = 1;
    }
    let sleeps = hit.status & status::SLEEP != 0;
    let (hp, mp) = (i32::from(hit.hp), i32::from(hit.mp));

    if power != 0 {
        let aimed = (draws.draw() % 100) as i32;
        let lucky = (draws.draw() % 100) as i32;
        if accuracy - evasion < aimed && !sleeps {
            result.outcome = outcome::MISS;
        } else {
            result.outcome = outcome::HIT;
            let mut amount = attack;
            if d.class != class::NONE {
                let level = i32::from(acting.level);
                let mut v = times(
                    (attack * (level + 3)).into(),
                    root((attack * 100 / defence).into()),
                );
                v = times(v / 10.0, power.into()) / 100.0 / 20.0;
                if lucky < critical - anti + i32::from(d.critical) && d.pair == 0 {
                    result.outcome = outcome::CRITICAL;
                    v = times(times(v, 7.0), 0.25);
                }
                if let Some(strength) = acting.strengths.get(usize::from(d.class)) {
                    let own = root(root((1250 * i32::from(*strength)).into()));
                    v = times(v, own / 10.0);
                }
                v = times(times(v, modifier.into()), 0.03125);
                amount = v as i32;
                if amount != 0 {
                    amount += i32::from(odd_frame);
                }
                if amount == 0 {
                    amount = 1;
                }
            }
            if hit.is_dead() && !(d.mode == 0 && u32::from(d.status) & status::DEAD != 0) {
                amount = 0;
            }
            let drains = d.class == class::DRAIN && amount > 0;
            match d.amount {
                0 => {
                    result.hp = amount;
                    if drains {
                        result.actor_hp = -amount.min(hp);
                    }
                }
                1 => {
                    amount /= 2;
                    result.mp = amount;
                    result.outcome = outcome::MP_HIT;
                    if d.class == class::DRAIN && amount > 0 {
                        result.actor_mp = -amount.min(mp);
                    }
                }
                2 => {
                    result.hp = amount;
                    result.outcome = outcome::HP_AND_MP;
                    result.mp = amount / 5 * 2;
                }
                3 => {
                    if ((draws.draw() % 32) as i32) < modifier {
                        result.hp = hp - 1;
                        result.outcome = outcome::CRITICAL;
                        result.clear |= status::SLEEP;
                    } else {
                        result.outcome = outcome::MISS;
                    }
                }
                _ => {}
            }
        }
    }
    // A hit wakes.
    if result.hp > 0 || result.mp > 0 {
        result.clear |= status::SLEEP;
    }
    // The status: not when the damage part missed.
    let bits = u32::from(d.status);
    if !(power != 0 && result.outcome == outcome::MISS) {
        let chance = (i32::from(d.rate) + na[8]) * nt[number::SHARE] / 100;
        match d.mode {
            3 => {
                let drawn = (draws.draw() % 100) as i32;
                let below = match d.class {
                    class::ALWAYS => 100,
                    _ => chance * modifier / 32,
                };
                if drawn < below {
                    result.add |= bits;
                } else if bits & BAD != 0 {
                    result.effects |= effect::REFUSED;
                }
            }
            0 if bits & SHIELD != 0 => result.add |= bits,
            0 => result.clear |= bits,
            _ => {}
        }
    }
    // The shield: one layer goes for every bad status that is tried.
    if hit.status & SHIELD != 0 && bits & BAD != 0 && d.mode == 3 {
        if hit.status & 0x0800 != 0 {
            result.clear |= 0x0800;
            result.effects |= effect::FIRST_LAYER;
        } else if hit.status & 0x1000 != 0 {
            result.clear |= 0x1800;
            result.effects |= effect::SECOND_LAYER;
        } else {
            result.clear |= SHIELD;
            result.effects |= effect::LAST_LAYER;
        }
        result.add &= !BAD;
        result.effects |= effect::REFUSED;
    }
    if result.hp == 0 && result.mp == 0 && result.add != 0 && result.clear != 0 {
        result.outcome = outcome::MISS;
    }
    if result.add & status::DEAD != 0 {
        result.hp = hp;
        result.outcome = outcome::HIT;
    }
    if sleeps {
        result.add &= !status::SLEEP;
    }
}

#[cfg(test)]
mod tests {
    use super::super::record::tests::{head, tables as made_up};
    use super::*;

    fn tables() -> Tables {
        let plain = [0, 0, 100, 0, 0xFF, 115, 0, 0, class::PHYSICAL, 0, 3, 0];
        Tables {
            plain_attack: Descriptor::parse(&plain).expect("a descriptor"),
            ..made_up()
        }
    }

    fn creature(stats: [u8; 4], growth: [u8; 6], level: u8) -> Record {
        let mut record = head(&"00".repeat(72));
        record.stats = [stats[0], stats[1], stats[2], stats[3], 0, 0];
        record.growth = growth;
        record.level = level;
        (record.hp, record.max_hp) = (100, 100);
        record.modifiers = [32; 12];
        record.equipment = [0xFF; 3];
        record
    }

    fn dogle() -> Record {
        creature([35, 35, 16, 26], [170, 170, 30, 100, 30, 100], 10)
    }

    fn terfrayd() -> Record {
        creature([30, 30, 16, 19], [150, 150, 40, 60, 110, 90], 7)
    }

    fn order<'a>(command: Command, actor: &'a Record, target: &'a Record) -> Order<'a> {
        Order {
            command,
            actor,
            actor_side: Side::Creature,
            target,
            target_side: Side::Creature,
            deva: NO_DEVA,
            lock: NO_DEVA,
            odd_frame: false,
        }
    }

    /// Resolves with the given draws and answers the result and how many were taken.
    fn with(tables: &Tables, order: &Order, given: &[u32]) -> (Resolved, usize) {
        let mut taken = 0;
        let mut draws = || {
            taken += 1;
            given[taken - 1]
        };
        let result = resolve(tables, order, &mut draws).expect("a result");
        (result, taken)
    }

    #[test]
    fn a_tie_of_a_product_goes_away_from_zero() {
        let step = f64::EPSILON;
        // 1.5 + 4.5 steps lies between two doubles: the machine takes the even one.
        assert_eq!((1.0 + 3.0 * step) * 1.5, 1.5 + 4.0 * step);
        assert_eq!(times(1.0 + 3.0 * step, 1.5), 1.5 + 5.0 * step);
        assert_eq!(times(-(1.0 + 3.0 * step), 1.5), -(1.5 + 5.0 * step));
        // Where the machine already goes away, and where nothing is cut, no change.
        assert_eq!(times(1.0 + step, 1.5), 1.5 + 2.0 * step);
        assert_eq!(times(3.0, 0.25), 0.75);
        assert_eq!(times(0.0, 5.0), 0.0);
    }

    #[test]
    fn the_rest_of_a_product_is_the_fused_one_without_the_fused_operation() {
        // Numbers of the size of a fight's (powers, strengths, shares of a root) and of
        // every size around them, against the processor's fused multiply-add.
        let mut seed = 0x1234_5678_9ABC_DEF0u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..200_000 {
            let mut number = || {
                let bits = next();
                let size = (bits % 64) as i32 - 32;
                let mantissa = 1.0 + (bits >> 11) as f64 / (1u64 << 53) as f64;
                let sign = if bits & 1 == 0 { 1.0 } else { -1.0 };
                sign * mantissa * 2f64.powi(size)
            };
            let (a, b) = (number(), number());
            let product = a * b;
            // (A rest of nothing may come with either sign; `times` takes both alike.)
            assert_eq!(
                product_rest(a, b, product),
                a.mul_add(b, -product),
                "{a} * {b}"
            );
        }
        for (a, b) in [
            (7.0, 0.25),
            (10.0 / 3.0, 3.0),
            (0.1, 0.2),
            (2f64.sqrt(), 2f64.sqrt()),
        ] {
            assert_eq!(product_rest(a, b, a * b), a.mul_add(b, -(a * b)));
        }
    }

    #[test]
    fn the_root_is_newton_s_and_not_the_machine_s() {
        assert_eq!(root(49.0), 7.0);
        assert_eq!(root(0.0), 0.0);
        assert_eq!(root(-4.0), 0.0);
        // The document's example: one bit below the nearest double.
        assert_eq!(root(2.0), 2f64.sqrt().next_down());
        assert_ne!(root(2.0), 2f64.sqrt());
        // The share of an element strength: 8 gives exactly 1, none gives nothing.
        assert_eq!(root(root(10000.0)) / 10.0, 1.0);
        assert_eq!(root(root(0.0)), 0.0);
    }

    #[test]
    fn a_plain_attack_of_a_creature() {
        let tables = tables();
        let (dogle, terfrayd) = (dogle(), terfrayd());
        let mut attack = order(Command::Attack, &dogle, &terfrayd);
        let (result, taken) = with(&tables, &attack, &[38, 58, 13]);
        let hit = Resolved {
            outcome: outcome::HIT,
            clear: status::SLEEP,
            hp: 24,
            deva: NO_DEVA,
            ..Resolved::default()
        };
        assert_eq!((result, taken), (hit, 3));
        // An odd frame adds one.
        attack.odd_frame = true;
        assert_eq!(with(&tables, &attack, &[38, 58, 13]).0.hp, 25);
        // 115 - 20 = 95: a draw of 96 misses, and then no status is drawn.
        attack.odd_frame = false;
        let (result, taken) = with(&tables, &attack, &[96, 0]);
        assert_eq!((result.outcome, result.hp, taken), (outcome::MISS, 0, 2));
        assert_eq!(result.effects, effect::MISSED);
        let (result, _) = with(&tables, &attack, &[95, 2, 0]);
        assert_eq!((result.outcome, result.hp), (outcome::HIT, 24));
        // A critical: the draw below 2. 24.5025 times 1.75.
        let (result, _) = with(&tables, &attack, &[0, 1, 0]);
        assert_eq!((result.outcome, result.hp), (outcome::CRITICAL, 42));
        // A sleeper is always hit and is woken; the dead take nothing.
        let mut asleep = terfrayd.clone();
        asleep.status = status::SLEEP;
        let sleeper = order(Command::Attack, &dogle, &asleep);
        let (result, _) = with(&tables, &sleeper, &[99, 58, 13]);
        assert_eq!(result.outcome, outcome::HIT);
        assert_eq!(result.clear, status::SLEEP);
        let mut dead = terfrayd.clone();
        dead.status = status::DEAD;
        let (result, taken) = with(&tables, &order(Command::Attack, &dogle, &dead), &[0, 50, 0]);
        assert_eq!((result.hp, result.clear, taken), (0, 0, 3));
    }

    #[test]
    fn a_creature_attacks_the_player() {
        let tables = tables();
        let terfrayd = terfrayd();
        let mut player = creature([20, 33, 10, 25], [0; 6], 3);
        let mut attack = order(Command::Attack, &terfrayd, &player);
        attack.target_side = Side::Player;
        assert_eq!(with(&tables, &attack, &[38, 58, 13]).0.hp, 10);
        player.status = status::DEFENDING;
        let mut attack = order(Command::Attack, &terfrayd, &player);
        attack.target_side = Side::Player;
        assert_eq!(with(&tables, &attack, &[38, 58, 13]).0.hp, 4);
        // The player without a weapon is not decided; with one he is.
        let mut armed = order(Command::Attack, &player, &terfrayd);
        armed.actor_side = Side::Player;
        assert!(resolve(&tables, &armed, &mut || 0u32).is_none());
        assert!(
            resolve(
                &tables,
                &order(Command::Spell(0), &player, &terfrayd),
                &mut || 0u32
            )
            .is_none()
        );
        assert!(
            resolve(
                &tables,
                &order(Command::Item(0), &player, &terfrayd),
                &mut || 0u32
            )
            .is_none()
        );
    }

    #[test]
    fn a_capture_is_one_draw_against_the_tier_s_row() {
        let tables = tables();
        let mut player = creature([20, 20, 20, 20], [0; 6], 5);
        let mut target = creature([20, 20, 20, 20], [100; 6], 5);
        (target.hp, target.max_hp) = (30, 30);
        let grade = |player: &Record, target: &Record, drawn: u32| {
            let (result, taken) = with(&tables, &order(Command::Capture, player, target), &[drawn]);
            assert_eq!(
                (result.outcome, result.clear, taken),
                (outcome::CAPTURE, status::SLEEP, 1)
            );
            result.grade
        };
        // Tier 0: 0, 0, 4, 16.
        assert_eq!(grade(&player, &target, 3), 2);
        assert_eq!(grade(&player, &target, 4), 3);
        // Level 8 against 6, HP 12 of 50, asleep: 4, then 8, then 64: tier 4.
        (player.level, target.level) = (8, 6);
        (target.hp, target.max_hp, target.status) = (12, 50, status::SLEEP);
        assert_eq!(grade(&player, &target, 14), 0);
        assert_eq!(grade(&player, &target, 15), 1);
        // Level 3 against 9, HP 20 of 100: 5 over 32 is nothing: tier 0 again.
        (player.level, target.level) = (3, 9);
        (target.hp, target.max_hp, target.status) = (20, 100, 0);
        assert_eq!(grade(&player, &target, 0), 2);
        // Awake at the same level with HP 10 of 100: 10 is not above the last step.
        (player.level, target.level, target.hp) = (5, 5, 10);
        assert_eq!(grade(&player, &target, 13), 0);
        assert_eq!(grade(&player, &target, 14), 1);
        // A target without HP counts as unhurt.
        target.hp = 0;
        assert_eq!(grade(&player, &target, 3), 2);
    }

    #[test]
    fn a_flight_and_the_commands_without_a_rule() {
        let tables = tables();
        let (dogle, mut terfrayd) = (dogle(), terfrayd());
        let flee = |target: &Record, drawn: u32| {
            let (result, taken) = with(&tables, &order(Command::Flee, &dogle, target), &[drawn]);
            assert_eq!((result.outcome, taken), (outcome::NOTHING, 1));
            result.grade
        };
        assert_eq!((flee(&terfrayd, 80), flee(&terfrayd, 81)), (0, 1));
        // A sleeper stops nobody.
        terfrayd.status = status::SLEEP;
        assert_eq!(flee(&terfrayd, 99), 0);
        let mut other = order(Command::Other, &dogle, &terfrayd);
        other.deva = 2;
        let (result, taken) = with(&tables, &other, &[]);
        let nothing = Resolved {
            deva: 2,
            ..Resolved::default()
        };
        assert_eq!((result, taken), (nothing, 0));
    }

    #[test]
    fn a_grade_and_the_target_s_body_change_a_body_attack() {
        let base = Descriptor::parse(&[0, 0, 240, 0, 0xFF, 240, 250, 100, 10, 0, 3, 0]);
        let base = base.expect("a descriptor");
        let entry = |attack: u8, grade: u8| Move {
            known: true,
            attack,
            grade,
            flag: 0,
        };
        // Grade 2: 30 more power, at most 255.
        assert_eq!(body_attack(base, &entry(0, 2), None).power, 255);
        // Grade 3: 30 more accuracy and 20 more rate, at most 255.
        let third = body_attack(base, &entry(0, 3), None);
        assert_eq!((third.accuracy, third.rate, third.power), (255, 255, 240));
        // Grade 4: 50 more on the critical byte, at most 125.
        assert_eq!(body_attack(base, &entry(0, 4), None).critical, 125);
        // "Rend Wings" against flag 2 and "Break Legs" against flag 1: 60 more.
        let low = Descriptor {
            critical: -20,
            ..base
        };
        assert_eq!(body_attack(low, &entry(15, 0), Some(2)).critical, 40);
        assert_eq!(body_attack(low, &entry(15, 0), Some(1)).critical, -20);
        assert_eq!(body_attack(low, &entry(14, 4), Some(1)).critical, 90);
        // Attack 12 has no power against flag 2, and a grade then gives it 30.
        assert_eq!(body_attack(base, &entry(12, 0), Some(2)).power, 0);
        assert_eq!(body_attack(base, &entry(12, 2), Some(3)).power, 30);
        assert_eq!(body_attack(base, &entry(12, 0), None), base);
    }
}
