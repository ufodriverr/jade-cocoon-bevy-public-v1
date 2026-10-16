
use std::path::PathBuf;

use jc_disc::{DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables;
use jc_formats::battle::duel::{Choice, Event, Options, Step};
use jc_formats::battle::record::{Draws, LEVEL, NOTHING, Random, Record};
use jc_formats::creatures::{Creature, state};
use jc_formats::items::{Keeping, Kind};
use jc_formats::versus::{self, Team};

/// The turns after which the side in turn gives up, so that a check always ends.
const MOST_TURNS: u32 = 5000;

/// The seed of the check, when the arguments ask for one.
pub fn asked(args: &[String]) -> Option<Result<u32, String>> {
    let value = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--duel-check="))?;
    Some(
        value
            .parse()
            .map_err(|_| format!("--duel-check=SEED takes a number, not {value}")),
    )
}

/// Runs the check.
pub fn run(seed: u32, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let exe = Executable::read(&mut disc)?;
    let tables =
        Tables::read(&exe.image, exe.base).ok_or("the fights' tables are not in the executable")?;
    let teams = [
        team_of(&tables, args, "--versus-save=", 0)?,
        team_of(&tables, args, "--versus-save2=", 1)?,
    ];
    let options = Options {
        items: option("--duel-rules=") != Some("original"),
        changing: option("--duel-changing=") != Some("off"),
    };
    for (side, team) in teams.iter().enumerate() {
        let members: Vec<String> = (team.members.iter())
            .map(|creature| {
                let record = &creature.record;
                format!(
                    "species {} level {} HP {} MP {}",
                    record.species, record.level, record.max_hp, record.max_mp
                )
            })
            .collect();
        println!(
            "duel check: side {side}: {}; items {:?}; plausible {}; team hash {:016x}",
            members.join(", "),
            team.items,
            team.plausible(&tables),
            team.hash()
        );
    }
    let mut duel = versus::start(&tables, [&teams[0], &teams[1]], options, seed);
    println!(
        "duel check: seed {seed}, {options:?}, dice {:?}, head start side {}{}",
        duel.start.rolls,
        duel.start.winner,
        match duel.start.equally_fast {
            true => ", equally fast",
            false => "",
        }
    );
    let mut picker = Random {
        seed: seed ^ 0x9E37_79B9,
    };
    let mut seen = duel.events.len();
    let (winner, gave_up) = loop {
        match duel.next(&tables) {
            Step::Over { winner, gave_up } => break (winner, gave_up),
            Step::Choose { side, turn } => {
                let choice = match turn > MOST_TURNS {
                    true => Choice::GiveUp,
                    false => {
                        let legal = duel.legal(&tables, side);
                        legal[picker.draw() as usize % legal.len()]
                    }
                };
                let gauges = duel.gauges;
                duel.act(&tables, side, choice)
                    .map_err(|refusal| format!("turn {turn}: {choice:?} refused: {refusal:?}"))?;
                let happened: Vec<String> =
                    (duel.events[seen..].iter()).filter_map(shown).collect();
                seen = duel.events.len();
                let points: Vec<String> = (0..2)
                    .map(|side| {
                        duel.fighting(side).map_or("-".into(), |record| {
                            format!(
                                "{}/{} {}/{}",
                                record.hp, record.max_hp, record.mp, record.max_mp
                            )
                        })
                    })
                    .collect();
                println!(
                    "turn {turn}: side {side} {choice:?}; {}; HP/MP {} vs {}; gauges {gauges:?}; hash {:016x}",
                    happened.join(", "),
                    points[0],
                    points[1],
                    duel.state_hash()
                );
            }
        }
    };
    println!(
        "duel check: side {winner} won{} after {} turns and {} commands; final hash {:016x}",
        match gave_up {
            true => " (the other gave up)",
            false => "",
        },
        duel.turn(),
        duel.acts(),
        duel.state_hash()
    );
    Ok(())
}

/// The team of the save that the flag `name` (`--versus-save=N`) numbers in the first
/// card of `--card-dir=DIR` (by default the game folder's saves): its first three minions,
/// carried before kept, and its items. Without the flag, the built-in team of `fallback`.
pub(crate) fn team_of(
    tables: &Tables,
    args: &[String],
    name: &str,
    fallback: usize,
) -> Result<Team, Box<dyn std::error::Error>> {
    let option = |name: &str| args.iter().find_map(|arg| arg.strip_prefix(name));
    let Some(slot) = option(name) else {
        return Ok(built_in(tables, fallback));
    };
    let folder = option("--card-dir=").map_or_else(crate::paths::saves, PathBuf::from);
    let slot: usize = slot
        .parse()
        .map_err(|_| format!("{name}N takes a number"))?;
    let path = crate::card_view::file_of(&folder, 0, slot);
    let buffer = std::fs::read(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    let memory = versus::memory_of(&buffer).ok_or(format!("{}: not a save", path.display()))?;
    let picks = versus::first_three(&memory);
    Team::from_memory(tables, &memory, &picks)
        .map_err(|err| format!("{}: {err:?}", path.display()).into())
}

/// An event of a turn as the log shows it; the choice itself is logged apart.
fn shown(event: &Event) -> Option<String> {
    Some(match event {
        Event::Chose { .. } => return None,
        Event::Resolved {
            target,
            result,
            reaction,
            ..
        } => format!(
            "slot {target} outcome {} HP {} MP {} reaction {reaction}",
            result.outcome, result.hp, result.mp
        ),
        other => format!("{other:?}"),
    })
}

/// The minions of a built-in team, made by the set-up of records of the disc's tables
/// (species and level), carried, and its items.
fn built_in_parts(tables: &Tables, side: usize) -> (Vec<Creature>, Keeping) {
    let mut random = Random {
        seed: 0x5EED_0000 + side as u32,
    };
    let picks: [(u32, u32); 3] = match side {
        0 => [(4, 12), (21, 10), (59, 14)],
        _ => [(12, 13), (100, 11), (33, 9)],
    };
    let creatures: Vec<Creature> = (picks.iter())
        .filter_map(|&(species, level)| {
            Record::make(tables, species, (level - 1) * LEVEL, &mut random)
        })
        .map(|record| Creature::of_a_caught(state::CARRIED, record))
        .collect();
    let mut keeping = Keeping::new();
    for (number, count) in [(9, 3), (13, 1), (14, 2)] {
        keeping.give(number, count, Kind::Item, [NOTHING; 3]);
    }
    (creatures, keeping)
}

/// `--write-versus-saves=DIR`: the saves of the two built-in teams, written as the
/// memory card screen writes a save, in the slots 6 and 7 of the first card of DIR
/// (`card1/save05.dat`, `card1/save06.dat`), so that a check of the lobby has saves with
/// minions without the player's own. The two of each team are carried and its third is
/// kept, as a save of the story has them. Gives the files written.
pub fn write_saves(folder: &std::path::Path) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let config = LocalConfig::load()?;
    let mut disc = DiscImage::open(&config.disc_image)?;
    let exe = Executable::read(&mut disc)?;
    let tables =
        Tables::read(&exe.image, exe.base).ok_or("the fights' tables are not in the executable")?;
    let settings = crate::settings::Settings::default().0;
    let mut written = Vec::new();
    for (side, slot) in [(0, 5), (1, 6)] {
        let (creatures, keeping) = built_in_parts(&tables, side);
        let mut carried = jc_formats::carried::Carried::new_game();
        for (n, creature) in creatures.into_iter().enumerate() {
            let kept = match n {
                2 => state::KEPT,
                _ => state::CARRIED,
            };
            carried
                .creatures
                .put(kept, creature.record)
                .ok_or("no free record")?;
        }
        carried.keeping = keeping;
        let buffer = crate::saving::of_a_game(settings, &carried)?;
        let path = crate::card_view::file_of(folder, 0, slot);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, buffer)?;
        written.push(path);
    }
    Ok(written)
}

/// The built-in teams: three minions of the set-up each, and a few items.
fn built_in(tables: &Tables, side: usize) -> Team {
    let (creatures, keeping) = built_in_parts(tables, side);
    let memory = versus::memory_with(&creatures, &keeping);
    let picks: Vec<usize> = (0..creatures.len()).collect();
    Team::from_memory(tables, &memory, &picks).unwrap_or_else(|_| Team {
        members: creatures,
        items: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_check_is_asked_with_a_seed() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(asked(&args(&["jc-game", "--smoke=5"])), None);
        assert_eq!(asked(&args(&["jc-game", "--duel-check=42"])), Some(Ok(42)));
        assert!(matches!(
            asked(&args(&["jc-game", "--duel-check=x"])),
            Some(Err(_))
        ));
    }
}
