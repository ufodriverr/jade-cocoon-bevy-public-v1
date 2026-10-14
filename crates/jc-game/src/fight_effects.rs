
use std::fs::File;

use jc_disc::{Archive, DiscImage, Executable, LocalConfig};
use jc_formats::battle::Tables;
use jc_formats::battle::effects::{self, Asks, Body, Effects, Entry, Field, Who};
use jc_formats::battle::fight::{PLAYER, SLOTS};
use jc_formats::battle::record::Draws;
use jc_formats::model::Worn;
use jc_formats::package::Package;
use jc_formats::player::arms::{self, Effect, Sign};

use crate::scripts::{ActorEffect, Scripts};
use crate::shown::Standing;
use crate::walker::Walker;

/// The effect that runs in a fight.
pub struct Playing {
    /// Its number in the loader's table and its package.
    pub number: usize,
    pub package: Package,
    pub effects: Effects,
    /// The library's table that brings a way to the length 4096, for the rings of a
    /// light.
    pub unit: effects::Unit,
    /// The acting party: its place among the actors of the room, none for the player.
    actor: Option<usize>,
    /// The targets in the order of their slots: the place of each among the actors of
    /// the room, none for the player himself.
    targets: Vec<Option<usize>>,
    /// The game frames it has been passed.
    passed: u32,
    /// The routines without a case have been named in the log.
    told: bool,
}

struct Fighters<'a> {
    player: &'a Standing,
    actors: &'a [Standing],
    actor: Option<usize>,
    targets: &'a [Option<usize>],
}

impl Fighters<'_> {
    fn party(&self, who: Who) -> &Standing {
        let actor = match who {
            Who::Actor => self.actor,
            Who::Target(target) => self.targets.get(target).copied().flatten(),
        };
        actor
            .and_then(|actor| self.actors.get(actor))
            .unwrap_or(self.player)
    }
}

impl Field for Fighters<'_> {
    fn point(&self, who: Who, bone: usize, place: [i32; 3]) -> [i32; 3] {
        let party = self.party(who);
        match party.pose.get(bone).copied().flatten() {
            Some(placement) => {
                let turned = placement.turn(place);
                std::array::from_fn(|k| party.place[k] + placement.position[k] + turned[k])
            }
            None => party.place,
        }
    }

    fn bone(&self, who: Who, bone: usize) -> [i32; 3] {
        self.point(who, bone, [0; 3])
    }

    fn stands(&self, who: Who) -> [i32; 3] {
        self.party(who).place
    }

    fn hand(&self) -> [i32; 3] {
        let he = self.player;
        match he.pose.get(effects::HAND_BONE).copied().flatten() {
            Some(placement) => std::array::from_fn(|k| he.place[k] + placement.position[k]),
            None => he.place,
        }
    }

    fn ground(&self) -> i32 {
        self.player.place[1]
    }

    fn arc(&self, across: i32, along: i32) -> i16 {
        self.player.arc.angle(across, along)
    }

    fn scale(&self, who: Who, _started: Option<[i32; 3]>) -> Option<[i32; 3]> {
        self.party(who).scale_in_use()
    }
}

/// The party of an effect that an actor is: the points of its model with the scales of
/// its record, and the byte of its slot in the effect block.
fn body(actor: &Standing, byte: u8) -> Body {
    Body {
        points: actor.points.clone(),
        bone_scales: actor.bone_scales.clone(),
        scale: actor.scale_in_use(),
        byte,
    }
}

/// The disc, for the numbers and the packages of the effects.
struct Source {
    disc: DiscImage<File>,
    archive: Archive,
    exe: Executable,
}

impl Source {
    fn open() -> Result<Self, Box<dyn std::error::Error>> {
        let config = LocalConfig::load()?;
        let mut disc = DiscImage::open(&config.disc_image)?;
        let archive = Archive::open(&mut disc)?;
        let exe = Executable::read(&mut disc)?;
        Ok(Self { disc, archive, exe })
    }

    fn number(&self, effect: Effect, weapon: u8) -> Option<usize> {
        let (image, base) = (&self.exe.image, self.exe.base);
        match effect {
            Effect::Spell(spell) => {
                let tables = Tables::read(image, base)?;
                Some(effects::of_spell(&tables.spells, spell as usize))
            }
            Effect::Attack => effects::Numbers::read(image, base)?.of_attack(weapon),
            Effect::Item(item) => Some(effects::Numbers::read(image, base)?.of_item(item)),
            _ => effect.number().and_then(|n| usize::try_from(n).ok()),
        }
    }

    /// The table of the library's routine that brings a way to the length 4096.
    fn unit(&self) -> Result<effects::Unit, Box<dyn std::error::Error>> {
        Ok(effects::Unit::from_image(&self.exe.image, self.exe.base)
            .ok_or("the library's table")?)
    }

    /// The package of an effect with its entry of the loader's table.
    fn package(&mut self, number: usize) -> Result<(Entry, Package), Box<dyn std::error::Error>> {
        let table = effects::table(&self.exe.image, self.exe.base).ok_or("the loader's table")?;
        let entry = effects::entry(&table, number).ok_or("no such effect")?;
        let package = Package::parse(&self.archive.read(&mut self.disc, entry.file)?)?;
        Ok((entry, package))
    }
}

/// The targets of a mask of slots in the order of the slots (`0x800182B8`): none for
/// the player himself, the actor that stands on the field for every other slot. A slot
/// of the own side but his own is a carried creature's figure. Stand-in: slot 0 is the
/// player also while his creature fights in his place.
fn targets_of(mask: u32, scripts: Option<&Scripts>) -> Vec<(usize, Option<usize>)> {
    (0..SLOTS)
        .filter(|slot| mask >> slot & 1 != 0)
        .filter_map(|slot| match slot {
            PLAYER => Some((slot, None)),
            _ => scripts
                .and_then(|scripts| scripts.stands_for(slot))
                .map(|actor| (slot, Some(actor))),
        })
        .collect()
}

/// The parties the targets are, each with the byte of its slot, and the targets
/// without their slots.
fn bodies_of(
    targets: Vec<(usize, Option<usize>)>,
    walker: &Walker,
    actors: &[Standing],
    bytes: [u8; SLOTS],
) -> (Vec<Body>, Vec<Option<usize>>) {
    let bodies = (targets.iter())
        .map(|&(slot, target)| {
            body(
                target.map_or(&walker.body, |actor| &actors[actor]),
                bytes[slot],
            )
        })
        .collect();
    (
        bodies,
        targets.into_iter().map(|(_, target)| target).collect(),
    )
}

impl Playing {
    pub fn start(
        sign: Sign,
        walker: &Walker,
        actors: &[Standing],
        scripts: Option<&Scripts>,
        half: &mut i16,
        kept: effects::Kept,
    ) -> Result<(Self, String), Box<dyn std::error::Error>> {
        let Sign::Effects {
            effect,
            loop_start,
            hold,
            loops,
            targets: mask,
        } = sign
        else {
            return Err("not the start of an effect".into());
        };
        let mut source = Source::open()?;
        let weapon = scripts.map_or(Worn::NOTHING, Scripts::weapon);
        let number = (source.number(effect, weapon))
            .ok_or_else(|| format!("{effect:?} with weapon {weapon} names no effect"))?;
        let he = &walker.player;
        let row = arms::row(he.action, he.group)
            .and_then(|row| walker.body.actions.get(row))
            .ok_or_else(|| format!("action {} has no row", he.action))?;

        let (entry, package) = source.package(number)?;
        let unit = source.unit()?;

        let later = arms::later_windows(row.windows, loop_start, hold, loops);
        let windows = effects::shifted(later, entry.shift);
        let bytes = scripts.map_or([0; SLOTS], Scripts::effect_bytes);
        let (bodies, targets) = bodies_of(targets_of(mask, scripts), walker, actors, bytes);
        let caught = matches!(effect, Effect::Capture(grade) if effects::catches(grade));
        if let (Effect::Attack, Some(left)) = (effect, effects::weapon_half_word(weapon)) {
            *half = left;
        }
        let mut effects = Effects::default().with_half(*half).with_kept(kept);
        effects.start(
            &package,
            windows,
            &body(&walker.body, bytes[PLAYER]),
            &bodies,
            caught,
        );
        let (tasks, _) = effects.living();
        let line = format!(
            "effect {number} ({effect:?}, file {}, {} images) starts with action {}: {tasks} tasks, the \
             windows {windows:?}, the targets {targets:?} of the slots {mask:#x}{}",
            entry.file,
            package.textures.len(),
            he.action,
            match effect {
                Effect::Capture(_) if caught => ", a capture that catches",
                Effect::Capture(_) => ", a capture that fails",
                _ => "",
            }
        );
        Ok((
            Self {
                number,
                package,
                effects,
                unit,
                actor: None,
                targets,
                passed: 0,
                told: false,
            },
            line,
        ))
    }

    pub fn start_actor(
        asked: ActorEffect,
        walker: &Walker,
        actors: &[Standing],
        scripts: Option<&Scripts>,
        half: &mut i16,
        kept: effects::Kept,
    ) -> Result<(Self, String), Box<dyn std::error::Error>> {
        let ActorEffect {
            actor,
            number,
            windows: times,
            targets: mask,
            half: left,
        } = asked;
        if let Some(left) = left {
            *half = left;
        }
        let acting = actors.get(actor).ok_or("no such actor")?;
        let mut source = Source::open()?;
        let (entry, package) = source.package(number)?;
        let unit = source.unit()?;
        let windows = effects::shifted(times, entry.shift);
        let bytes = scripts.map_or([0; SLOTS], Scripts::effect_bytes);
        let (bodies, targets) = bodies_of(targets_of(mask.into(), scripts), walker, actors, bytes);
        let mut effects = Effects::default().with_half(*half).with_kept(kept);
        effects.start(&package, windows, &body(acting, 0), &bodies, false);
        let (tasks, _) = effects.living();
        let line = format!(
            "effect {number} (file {}, {} images) starts for thing {}: {tasks} tasks, the \
             windows {windows:?}, the targets {targets:?} of the slots {mask:#x}",
            entry.file,
            package.textures.len(),
            actor + 1,
        );
        Ok((
            Self {
                number,
                package,
                effects,
                unit,
                actor: Some(actor),
                targets,
                passed: 0,
                told: false,
            },
            line,
        ))
    }

    /// One game frame of the effect's tasks (places 32 and 33 of the list of a fight,
    /// behind the player's task and the actors'). Answers what it asked of the sound,
    /// which nothing plays yet, and of the clip ([`Playing::clip_picture`]).
    pub fn pass(
        &mut self,
        player: &Standing,
        actors: &[Standing],
        draws: &mut impl Draws,
        loops_end: bool,
    ) -> Vec<Asks> {
        let field = Fighters {
            player,
            actors,
            actor: self.actor,
            targets: &self.targets,
        };
        (self.effects).pass(&self.package, &field, draws, loops_end);
        self.passed += 1;
        if !self.told && !self.effects.unread().is_empty() {
            self.told = true;
            bevy::log::warn!(
                "effect {}: the routines {:?} are not built, their tasks only count to their \
                 end",
                self.number,
                self.effects.unread()
            );
        }
        self.effects.take_asked()
    }

    pub fn clip_picture(&self) -> Option<&[u8]> {
        let picture = self.effects.picture()?;
        Some(self.package.clips.first()?.get(picture)?)
    }

    /// The game frames the effect has been passed.
    pub fn passed(&self) -> u32 {
        self.passed
    }
}
