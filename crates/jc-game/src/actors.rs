
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fs::File;
use std::sync::Arc;

use jc_disc::{Archive, DiscImage, Executable};
use jc_formats::action::{Point, Stage, model_scale, stage_of_level};
use jc_formats::animation::{Animation, Playback, WHOLE};
use jc_formats::command::{self, Runner};
use jc_formats::creatures;
use jc_formats::field::Ground;
use jc_formats::figure::Request;
use jc_formats::hue;
use jc_formats::mesh::{Kept, MeshBlock};
use jc_formats::model::{
    ModelRecord, ModelTable, actor_model, actor_turn, species_model, species_turns,
};
use jc_formats::mover::{self, ArcTangent, Mover, Path};
use jc_formats::scene::{
    Actor, ActorRecord, COMMAND_FIGHT, Creature, MODEL_SLOTS, ObjectList, PACKAGE_BASE,
    ScenePackage,
};
use jc_formats::skeleton::{CREATURE_SKELETON, Scales, Shape, Skeleton, bone_groups, rotation};
use jc_formats::tim::Tim;
use jc_formats::tint::{self, Tint, Tinted};
use jc_formats::vram::{CARRIED_PLACES, FIELD_PLACES, Place, Vram, place_tables};

use jc_formats::roamer::{Roamer, Routine, Routines};

use crate::shown::{Roams, Standing};

#[derive(Clone, Copy)]
pub struct MoveAsked {
    pub number: usize,
    pub loops: i16,
    pub size: i16,
}

#[derive(Default)]
pub struct Roamers {
    /// The models the room loader has noted for the species that roam, each with the
    /// turn of its colours (`roaming::noted`): they have the first slots of the field's
    /// loader, in front of the models of the room's own actors.
    pub noted: Vec<(u32, i32)>,
    /// The creatures the picking put into the room, each as the maker makes its actor
    /// ([`roamer`]).
    pub actors: Vec<Actor>,
    /// What the picking answered for each of them, and the slot of the room's word in
    /// what the game carries (area, place, slot): a fight with a creature needs both.
    pub stands: Vec<jc_formats::roaming::Stand>,
    pub slot: (usize, usize, i16),
    /// The opponents 1 and 2 of the creatures' groups, each with the number of its
    /// creature among `actors` and its own number in the fight ([`mate`]).
    pub mates: Vec<(usize, usize, Actor)>,
    /// The player's level, which picks the routine of a creature he has outgrown.
    pub player_level: u8,
    /// The creatures the player carries, in the order of their slots: each gets a
    /// figure for a summon ([`figure`]).
    pub figures: Vec<Figure>,
}

#[derive(Clone, Copy)]
pub struct Figure {
    pub species: u32,
    pub level: u8,
    pub strengths: [u8; 4],
    pub look: u8,
    pub line: [u8; creatures::LINE],
}

impl Figure {
    /// The figure of a creature the player has.
    pub fn of(creature: &creatures::Creature) -> Self {
        Self {
            species: creature.record.species,
            level: creature.record.level,
            strengths: creature.record.strengths,
            look: creature.line.look,
            line: creature.line.species,
        }
    }

    /// The model the figure is noted with and the turn of its colours
    /// (`creatures::colour_turn`, with the row of the look's species).
    fn noted(&self, exe: &Executable, arc: &ArcTangent) -> Option<(u32, i32)> {
        let model = species_model(&exe.image, exe.base, self.species)?;
        let row = species_turns(&exe.image, exe.base, self.look.into())?;
        Some((
            model.into(),
            creatures::colour_turn(self.strengths, row, arc),
        ))
    }
}

struct Job {
    request: Request,
    /// The second and the third model of the request, when anything is mixed.
    others: Option<[Other; 2]>,
    /// The textures, when they are another model's than the creature's own.
    textures: Option<Vec<Tim>>,
}

/// What the job reads of a model that is mixed into the creature's own: its mesh, its
/// stages, its first animation and the points of its head.
struct Other {
    mesh: MeshBlock,
    stages: Vec<Stage>,
    first: Animation,
    points: Vec<Point>,
}

impl Job {
    fn of(
        disc: &mut DiscImage<File>,
        archive: &Archive,
        exe: &Executable,
        models: &ModelTable,
        of: &Figure,
    ) -> Result<Self, String> {
        let model_of = |species| species_model(&exe.image, exe.base, species).map(u32::from);
        let request = Request::of(of.species, of.look, &of.line, model_of)
            .ok_or("a species of its line has no model")?;
        let mut read = |model: u32| -> Result<(&ModelRecord, Vec<u8>), String> {
            let record = (models.get(model as usize))
                .ok_or_else(|| format!("model {model} is not in the table"))?;
            let file = archive
                .read(disc, record.file as usize)
                .map_err(|err| format!("model file {}: {err}", record.file))?;
            Ok((record, file))
        };
        let mut other = |model: u32| -> Result<Other, String> {
            let (record, file) = read(model)?;
            let wrong = |err: &dyn std::fmt::Display| format!("model {model}: {err}");
            let head = record.head(&file).map_err(|err| wrong(&err))?;
            Ok(Other {
                mesh: record.mesh(&file).map_err(|err| wrong(&err))?,
                stages: head.stages().map_err(|err| wrong(&err))?,
                first: record.animation(&file, 0).map_err(|err| wrong(&err))?,
                points: head.points().map_err(|err| wrong(&err))?,
            })
        };
        let others = match request.is_mixed() {
            true => Some([other(request.models[1])?, other(request.models[2])?]),
            false => None,
        };
        let textures = match request.textures == request.models[0] {
            true => None,
            false => {
                let (record, file) = read(request.textures)?;
                let textures = record.textures(&file);
                Some(textures.map_err(|err| format!("model {}: {err}", request.textures))?)
            }
        };
        Ok(Self {
            request,
            others,
            textures,
        })
    }
}

pub fn figure(of: &Figure) -> Actor {
    let experience = u32::from(of.level.saturating_sub(1)) << 26;
    Actor {
        flags: 1 | 8,
        action: jc_formats::merge_screen::Form::ACTION,
        ..roamer(of.species, experience, [0; 4])
    }
}

pub fn form(
    disc: &mut DiscImage<File>,
    archive: &Archive,
    package: &ScenePackage,
    of: &Figure,
) -> Result<(String, Standing, Vram), Box<dyn std::error::Error>> {
    let exe = Executable::read(disc)?;
    let models = ModelTable::from_executable(&exe.image, exe.base)?;
    let mut tables = place_tables(&exe.image, exe.base)?;
    let own_places = tables.swap_remove(CARRIED_PLACES);
    let arc = ArcTangent::from_image(&exe.image, exe.base)
        .ok_or("the executable has no arc tangent table")?;
    let arc = Arc::new(arc);
    let noted = of.noted(&exe, &arc).ok_or("its species has no model")?;
    let mut memory = Memory {
        list: usize::MAX,
        noted: vec![noted],
        sent: Vec::new(),
        vram: Vram::new(),
    };
    let actor = Actor {
        flags: 8,
        ..figure(of)
    };
    let (line, standing) = stand(
        disc,
        archive,
        package,
        &exe,
        &models,
        (&actor, None, false, (false, false, false)),
        (&own_places, &arc),
        (&mut memory, Some(of)),
    )?;
    Ok((line, standing, memory.vram))
}

pub fn roamer(species: u32, experience: u32, [x, y, z, angle]: [i32; 4]) -> Actor {
    Actor {
        view_mask: u32::MAX,
        instance: 0,
        spawner: 0,
        record: ActorRecord {
            address: 0,
            name: 0,
            model: 0,
            bone_table: 0,
            model_scale: 0,
            bone_scales: 0,
            list: 0,
            fade: 4,
            word: 0,
            turn: 0,
            mirror: 0,
        },
        creature: Some(Creature {
            species,
            word: experience,
        }),
        place: [x, y, z],
        angle: angle as i16,
        action: 1,
        set: None,
        flags: 0,
    }
}

pub fn mate(species: u32, experience: u32) -> Actor {
    Actor {
        flags: 1 | 8,
        action: jc_formats::merge_screen::Form::ACTION,
        ..roamer(species, experience, [0; 4])
    }
}

/// The video memory of one object list: the models of its actors, each sent to the place
/// of its slot.
struct Memory {
    /// The object list, by its place in `ScenePackage::object_lists`.
    list: usize,
    /// The different models of the list in the order of their slots, each with the turn of
    /// its colours: that of the first actor of the list that has the model.
    noted: Vec<(u32, i32)>,
    /// The slots whose images have been sent.
    sent: Vec<usize>,
    vram: Vram,
}

/// The actors of a scene package as they stand in the room, and the video memories their
/// models are drawn from ([`Standing::memory`]): one per object list with actors, which is
/// one for the room unless its package picks a list by the state of the story; with `only`
/// the room has the actors of that list alone. Adds a line
/// about each actor to `lines`; an actor that cannot be shown has only its line. With
/// `creature_move` every creature among them is given that move of the creatures' table and
/// told to go on from its hold, as a look at what a command list does to an actor. With
/// `whole` an actor keeps every animation the game has in memory for it (the resident ones
/// and those of its set), for the actions a script orders. The creatures of `roamers`
/// stand behind the room's own actors, so that the number a script names an actor by
/// stays the actor's.
#[allow(clippy::too_many_arguments)]
pub fn standing(
    disc: &mut DiscImage<File>,
    archive: &Archive,
    package: &ScenePackage,
    creature_move: Option<MoveAsked>,
    whole: bool,
    only: Option<usize>,
    roamers: &Roamers,
    lines: &mut Vec<String>,
) -> Result<(Vec<Standing>, Vec<Vram>), Box<dyn std::error::Error>> {
    let exe = Executable::read(disc)?;
    let models = ModelTable::from_executable(&exe.image, exe.base)?;
    let mut tables = place_tables(&exe.image, exe.base)?;
    let own_places = tables.swap_remove(CARRIED_PLACES);
    let places = tables.swap_remove(FIELD_PLACES);
    let arc = ArcTangent::from_image(&exe.image, exe.base)
        .ok_or("the executable has no arc tangent table")?;
    let arc = Arc::new(arc);
    let lists = package.object_lists();
    let read_routines = package.read_routines();
    // The things the fights of the room's scripts name as opponents: a fight asks them
    // for moves whose animations are not in memory.
    let fighters: Vec<usize> = (package.script_commands().iter())
        .filter(|command| command.code == COMMAND_FIGHT)
        .filter_map(|command| package.fight_things(command.c))
        .flat_map(|things| things.into_iter().flatten())
        .map(|thing| thing as usize)
        .collect();
    let mut memories: Vec<Memory> = Vec::new();
    let mut standing = Vec::new();
    for (number, actor) in package.actors().iter().enumerate() {
        // The first list that holds the actor is the one it is drawn with.
        let held = |list: &jc_formats::scene::ObjectList| list.has_actor(actor.instance);
        let Some(list) = lists.iter().position(held) else {
            lines.push(format!("actor {number}: not shown: in no object list"));
            continue;
        };
        // The room uses one list: an actor of another is not made.
        if only.is_some_and(|only| !lists.get(only).is_some_and(held)) {
            lines.push(format!("actor {number}: not in the room's object list"));
            continue;
        }
        let list = only.unwrap_or(list);
        let memory = memory_of(&mut memories, list, lists.get(list), |list| {
            noted(package, &exe, &roamers.noted, list)
        });
        // A script's routine names an actor by its number among those of the room.
        let reads =
            whole && (read_routines.iter()).any(|routine| routine.actor as usize == standing.len());
        let fights = whole && fighters.contains(&(standing.len() + 1));
        let stood = stand(
            disc,
            archive,
            package,
            &exe,
            &models,
            (actor, creature_move, whole, (reads, fights, false)),
            (&places, &arc),
            (&mut memories[memory], None),
        );
        match stood {
            Ok((line, mut actor)) => {
                lines.push(format!("actor {number}: {line}"));
                actor.memory = memory;
                standing.push(actor);
            }
            Err(why) => lines.push(format!("actor {number}: not shown: {why}")),
        }
    }
    for tinted in Tinted::of(package) {
        let Some(mix) = Tint::at(&exe.image, exe.base, tinted.record) else {
            lines.push(format!(
                "an object tints the things of mask {:#x}, but {:#010x} is no record",
                tinted.mask, tinted.record
            ));
            continue;
        };
        for thing in tinted.things() {
            let number = thing as usize - 1;
            let Some(actor) = (standing.get_mut(number))
                .filter(|actor| memories[actor.memory].list == tinted.list)
            else {
                continue;
            };
            let Some(place) = places.get(actor.slot) else {
                continue;
            };
            let down = place.large_palette[1];
            let from = (
                tint::SOURCE.0,
                tint::SOURCE.1.wrapping_add_signed(down.into()),
            );
            let to = (tint::COPY.0, tint::COPY.1 + number);
            let vram = &mut memories[actor.memory].vram;
            let row: Vec<u16> = (0..tint::COLOURS)
                .map(|colour| vram.word(from.0 + colour, from.1))
                .collect();
            vram.send(to, tint::COLOURS, &mix.row(&row));
            actor.runner.tint(tint::rows(number, down));
            lines.push(format!(
                "actor {number}: tinted by the record {:#010x} (towards {:?}, {} of 4096 grey, \
                 {} of 4096 of the colour): the palette row {} of its model's slot {} is \
                 copied to row {}",
                tinted.record, mix.colour, mix.grey, mix.share, from.1, actor.slot, to.1
            ));
        }
    }
    let routines = match roamers.actors.is_empty() {
        true => None,
        false => {
            let engine = archive.read(disc, jc_formats::rooms::FIELD_ENGINE_FILE)?;
            Some(Routines::from_field_engine(&engine)?)
        }
    };
    // Which actor of the room each creature has become, for its group's others.
    let mut roaming: Vec<Option<usize>> = vec![None; roamers.actors.len()];
    for (number, actor) in roamers.actors.iter().enumerate() {
        let list = only.unwrap_or(0);
        let memory = memory_of(&mut memories, list, lists.get(list), |list| {
            noted(package, &exe, &roamers.noted, list)
        });
        let stood = stand(
            disc,
            archive,
            package,
            &exe,
            &models,
            // A touch makes it the opponent of a fight, whose moves read their
            // animations: it has the shelf of an opponent.
            (actor, None, true, (false, true, true)),
            (&places, &arc),
            (&mut memories[memory], None),
        );
        match stood.and_then(|stood| {
            let stand = roamers
                .stands
                .get(number)
                .ok_or("the picking has no answer")?;
            Ok((stood, *stand))
        }) {
            Ok(((line, mut made), stand)) => {
                let address = (routines.as_ref().zip(actor.creature)).and_then(|(table, is)| {
                    table.address(is.species, is.level(), roamers.player_level)
                });
                let routine = address.and_then(Routine::at);
                lines.push(format!(
                    "roaming creature {number}: actor {}, routine {:#010x} ({routine:?}); {line}",
                    standing.len(),
                    address.unwrap_or(0)
                ));
                made.memory = memory;
                made.roams = Some(Roams {
                    routine,
                    record: Roamer::new(),
                    stand,
                    slot: roamers.slot,
                    stood: None,
                    held: false,
                });
                roaming[number] = Some(standing.len());
                standing.push(made);
            }
            Err(why) => lines.push(format!("roaming creature {number}: not made: {why}")),
        }
    }
    // The others of a creature's group stand behind the creatures, hidden until a fight
    // with their creature. Their models are among those noted for the species.
    for (number, opponent, actor) in &roamers.mates {
        let Some(creature) = roaming.get(*number).copied().flatten() else {
            continue;
        };
        let list = only.unwrap_or(0);
        let memory = memory_of(&mut memories, list, lists.get(list), |list| {
            noted(package, &exe, &roamers.noted, list)
        });
        let stood = stand(
            disc,
            archive,
            package,
            &exe,
            &models,
            (actor, None, true, (false, true, true)),
            (&places, &arc),
            (&mut memories[memory], None),
        );
        match stood {
            Ok((line, mut made)) => {
                lines.push(format!(
                    "roaming creature {number}: opponent {opponent} of its group is actor {}; \
                     {line}",
                    standing.len()
                ));
                made.memory = memory;
                made.mate = Some((creature, *opponent));
                standing.push(made);
            }
            Err(why) => lines.push(format!(
                "roaming creature {number}: opponent {opponent} of its group is not made: {why}"
            )),
        }
    }
    // The figures of the carried creatures stand behind everything else. The game has
    // one place in video memory for the one that is on the field (the first slot of the
    // job's own loader) and reads its model when it is summoned; here every figure is
    // read with the room into a video memory of its own, so a summon reads nothing.
    for (number, carried) in roamers.figures.iter().enumerate() {
        let Some(noted) = carried.noted(&exe, &arc) else {
            lines.push(format!(
                "carried creature {number}: no figure: its species has no model"
            ));
            continue;
        };
        memories.push(Memory {
            list: usize::MAX - number,
            noted: vec![noted],
            sent: Vec::new(),
            vram: Vram::new(),
        });
        let memory = memories.len() - 1;
        let stood = stand(
            disc,
            archive,
            package,
            &exe,
            &models,
            (&figure(carried), None, true, (false, true, false)),
            (&own_places, &arc),
            (&mut memories[memory], Some(carried)),
        );
        match stood {
            Ok((line, mut made)) => {
                lines.push(format!(
                    "carried creature {number}: actor {}, a video memory of its own; {line}",
                    standing.len()
                ));
                made.memory = memory;
                made.carried = Some(number);
                standing.push(made);
            }
            Err(why) => lines.push(format!("carried creature {number}: no figure: {why}")),
        }
    }
    let vrams = memories.into_iter().map(|memory| memory.vram).collect();
    Ok((standing, vrams))
}

/// The models of an object list in the order of the loader's slots, each with the turn
/// of its colours: those the room loader noted for the species that roam (`before`), and
/// behind them those the package's entry routine notes for the list's actors, as far as
/// the loader has slots.
fn noted(
    package: &ScenePackage,
    exe: &Executable,
    before: &[(u32, i32)],
    list: Option<&ObjectList>,
) -> Vec<(u32, i32)> {
    let mut noted = before.to_vec();
    let own = list
        .map(|list| package.noted_actors(list, |actor| actor_model(&exe.image, exe.base, actor)));
    for (model, actor) in own.unwrap_or_default() {
        if noted.len() == MODEL_SLOTS {
            break;
        }
        if noted.iter().all(|(known, _)| *known != model) {
            let turn = actor_turn(&exe.image, exe.base, &actor);
            noted.push((model, turn.unwrap_or(0)));
        }
    }
    noted
}

/// The video memory of an object list among `memories`, made with what `noted` answers
/// for the list when no actor before has asked for it.
fn memory_of<'a>(
    memories: &mut Vec<Memory>,
    list: usize,
    objects: Option<&'a ObjectList>,
    noted: impl Fn(Option<&'a ObjectList>) -> Vec<(u32, i32)>,
) -> usize {
    if let Some(memory) = memories.iter().position(|memory| memory.list == list) {
        return memory;
    }
    memories.push(Memory {
        list,
        noted: noted(objects),
        sent: Vec::new(),
        vram: Vram::new(),
    });
    memories.len() - 1
}

/// Hands the actors the ground of the room (its tables 3 and 4), which the mover of an actor
/// with flag 0x40000 ends on in every game frame. Adds a line about every actor: where the
/// mover would leave it on the place it is made at, or that it does not walk.
pub fn on_the_ground(actors: &mut [Standing], ground: &Arc<Ground>, lines: &mut Vec<String>) {
    for (number, actor) in actors.iter_mut().enumerate() {
        actor.ground = Some(ground.clone());
        if actor.mover.flags & mover::flag::WALKS == 0 {
            lines.push(format!("actor {number}: does not walk over the ground"));
            continue;
        }
        let mut place = actor.place;
        lines.push(
            match ground.walk_actor(&mut place, actor.place, actor.angle, &actor.arc) {
                Ok(answer) => format!(
                    "actor {number}: walks over the ground, which puts it at {place:?} \
                     (answer {answer})"
                ),
                Err(fault) => format!("actor {number}: walks, but off the ground: {fault}"),
            },
        );
    }
}

#[derive(Clone, Copy, Default)]
pub struct Ordered {
    /// Every actor is carried by its animation (flag 0x01000000, order kind 8).
    pub carried: bool,
    /// Every actor walks over the ground (flag 0x40000, order kind 7).
    pub walks: bool,
    /// This actor is put on this path of the room's kind 4 resource (order kind 11).
    pub path: Option<(usize, usize)>,
}

/// Gives the actors what [`Ordered`] asks, with a line about it. `paths` are those of the
/// room and `ground` its tables 3 and 4.
pub fn ordered(
    actors: &mut [Standing],
    asked: Ordered,
    paths: &[Path],
    ground: Option<Ground>,
    lines: &mut Vec<String>,
) {
    if let (true, Some(ground)) = (asked.walks, ground) {
        let ground = Arc::new(ground);
        lines.push(format!(
            "{} actors walk over the ground: {} triangles in table 3, {} barriers in table 4",
            actors.len(),
            ground
                .regions
                .first()
                .map_or(0, |region| region.triangles.len()),
            ground.barriers.len()
        ));
        for (number, actor) in actors.iter_mut().enumerate() {
            actor.mover.flags |= mover::flag::WALKS;
            actor.ground = Some(ground.clone());
            // Where the walk-mesh mover would leave the actor if it made no step.
            let mut place = actor.place;
            let answer = ground.walk_actor(&mut place, actor.place, actor.angle, &actor.arc);
            lines.push(match answer {
                Ok(answer) => format!(
                    "actor {number} at {:?}: answer {answer}, put at {place:?}",
                    actor.place
                ),
                Err(fault) => format!("actor {number} at {:?}: {fault}", actor.place),
            });
        }
    }
    if asked.carried {
        for actor in actors.iter_mut() {
            actor.mover.flags |= mover::flag::CARRIED;
        }
        lines.push(format!(
            "{} actors are carried by their animation",
            actors.len()
        ));
    }
    let Some((actor, list)) = asked.path else {
        return;
    };
    lines.push(match (actors.get_mut(actor), paths.get(list)) {
        (Some(standing), Some(path)) => {
            standing.mover.set_path(path.clone());
            standing.runner.hidden = false;
            format!(
                "actor {actor} is on path {list}: {} points, places divided by {}",
                path.points.len(),
                path.divisor
            )
        }
        (None, _) => format!("no path given: {} actors are shown", actors.len()),
        (_, None) => format!("no path given: the room has {} paths", paths.len()),
    });
}

/// Gives an actor a move of the creatures' move table and tells it to go on from the hold
/// the move starts with. `read` gives an animation of the actor's model, as the game's
/// loader does for the list's own read commands: the animations of the actions the list
/// starts are kept with the actor.
fn given(
    actor: &mut Standing,
    exe: &Executable,
    asked: MoveAsked,
    read: impl Fn(usize) -> Result<Animation, String>,
) -> Result<(), String> {
    let images = [(&exe.image[..], exe.base)];
    let moves = command::move_table(&images, command::CREATURE_MOVES);
    let list = (moves.get(asked.number))
        .and_then(|address| command::list_in(&images, *address))
        .ok_or_else(|| format!("the table has {} moves", moves.len()))?;
    for played in &list {
        let command::Kind::Action(action) = played.kind else {
            continue;
        };
        let animation = usize::try_from(action)
            .ok()
            .and_then(|action| actor.actions.get(action)?.animation)
            .ok_or_else(|| format!("the model has no action {action} with an animation"))?;
        if let Entry::Vacant(kept) = actor.animations.entry(animation) {
            kept.insert(read(animation)?);
        }
    }
    actor.runner.set_list(list);
    (actor.runner.go_on, actor.runner.moving) = (true, true);
    (actor.runner.request_loops, actor.runner.request_size) = (asked.loops, asked.size);
    // The creatures of the scene packages are hidden until a script shows them (order
    // `0x1A`), and only move 17 of the table shows the actor itself: for the look at a move
    // the order is taken as given.
    actor.runner.hidden = false;
    Ok(())
}

/// One actor as it stands, with a line about it, or why it cannot be shown. Its model is
/// moved to the place of its slot of the field's loader (`places`) and its images are sent
/// to the list's video memory when no actor before it brought them. The figure of a
/// creature the player has (`of`) is what the model job makes of its models.
#[allow(clippy::too_many_arguments)]
fn stand(
    disc: &mut DiscImage<File>,
    archive: &Archive,
    package: &ScenePackage,
    exe: &Executable,
    models: &ModelTable,
    (actor, creature_move, whole, (reads, fights, roams)): (
        &Actor,
        Option<MoveAsked>,
        bool,
        (bool, bool, bool),
    ),
    (places, arc): (&[Place], &Arc<ArcTangent>),
    (memory, of): (&mut Memory, Option<&Figure>),
) -> Result<(String, Standing), String> {
    let job = match of {
        Some(of) => Some(Job::of(disc, archive, exe, models, of)?),
        None => None,
    };
    let model = actor_model(&exe.image, exe.base, actor)
        .ok_or_else(|| format!("its species has no model: {:?}", actor.creature))?;
    let slot = (memory.noted.iter())
        .position(|(noted, _)| *noted == model)
        .ok_or_else(|| format!("model {model} has none of the loader's slots"))?;
    let place = (places.get(slot)).ok_or_else(|| format!("the loader has no slot {slot}"))?;
    let turn = memory.noted[slot].1;
    let model = model as usize;
    let record = (models.get(model)).ok_or_else(|| format!("model {model} is not in the table"))?;
    let file = archive
        .read(disc, record.file as usize)
        .map_err(|err| format!("model file {}: {err}", record.file))?;
    let wrong =
        |err: &dyn std::fmt::Display| format!("model {model} (file {}): {err}", record.file);

    // The frame: the first one of the action the instance names.
    let head = record.head(&file).map_err(|err| wrong(&err))?;
    let action = usize::try_from(actor.action)
        .ok()
        .and_then(|action| head.actions.get(action))
        .ok_or_else(|| wrong(&format!("no action {}", actor.action)))?;
    let number = action
        .animation
        .ok_or_else(|| wrong(&format!("action {} has no animation", actor.action)))?;
    // The game has an animation in memory when it is resident or in the set the scene
    // loads for the actor.
    let resident = record.animations.get(number).is_some_and(|a| a.resident);
    let set: Vec<usize> = match actor.set {
        Some(set) => (record.set(&file, set as usize).map_err(|err| wrong(&err))?)
            .iter()
            .map(|entry| entry.animation)
            .collect(),
        None => Vec::new(),
    };
    let in_set = set.contains(&number);
    if !resident && !in_set {
        return Err(wrong(&format!(
            "animation {number} of action {} is neither resident nor in set {:?}",
            actor.action, actor.set
        )));
    }
    let animation = record.animation(&file, number).map_err(|err| wrong(&err))?;
    // The spawner starts the actor at the action's first frame, one frame per game frame.
    let mut runner = Runner::new(
        Playback::new(number, action.span(), WHOLE as i16),
        actor.action as i16,
        actor.record.fade as i16,
    );
    runner.hidden = actor.flags & 1 != 0;
    let frame = runner.playback.frame(animation.frames);

    // The bone table and the scales.
    let defaults = package.actor_defaults(actor.spawner);
    // A figure has the frame the model job makes for its skeleton, and the points of
    // its head mixed.
    let mut shape: Option<Shape> = None;
    let mut points = head.points().unwrap_or_default();
    // The words the stages of the three models of a mix have for each mesh object.
    let mut words: [Vec<i32>; 3] = Default::default();
    let (skeleton, table, model_scale, bone_scales) = match actor.creature {
        Some(creature) => {
            let skeleton =
                Skeleton::at(&exe.image, exe.base, CREATURE_SKELETON).map_err(|err| wrong(&err))?;
            let level = creature.level();
            let number = stage_of_level(&exe.image, exe.base, level)
                .ok_or_else(|| wrong(&format!("a creature of level {level} has no stage")))?;
            let stages = head.stages().map_err(|err| wrong(&err))?;
            let mut stage = (stages.get(number).cloned()).ok_or_else(|| {
                wrong(&format!(
                    "no stage {number} for level {level}: {} stages",
                    stages.len()
                ))
            })?;
            // A figure of several models has the scales of their stages by their
            // weights, and shows a bone group that one of them shows.
            let mut mixed = String::new();
            if let Some(job) = &job {
                let request = &job.request;
                if let Some([second, third]) = &job.others
                    && let (Some(second), Some(third)) =
                        (second.stages.get(number), third.stages.get(number))
                {
                    stage.scale = request.scale([stage.scale, second.scale, third.scale]);
                    words = [
                        stage.shares.clone(),
                        second.shares.clone(),
                        third.shares.clone(),
                    ];
                    jc_formats::figure::shown(&mut stage.shown, [second, third]);
                }
                let first = record.animation(&file, 0).map_err(|err| wrong(&err))?;
                let standing = record.animation(&file, 1).map_err(|err| wrong(&err))?;
                let firsts = match &job.others {
                    Some([second, third]) => [&first, &second.first, &third.first],
                    None => [&first; 3],
                };
                let mut made = (request.shape(&skeleton, firsts))
                    .ok_or_else(|| wrong(&"a first animation has too few bones"))?;
                if let Some([second, third]) = &job.others {
                    request.points(&mut points, [&second.points, &third.points]);
                }
                let height = jc_formats::figure::ground(
                    &mut made,
                    &skeleton,
                    &standing,
                    &stage.bones,
                    &points,
                );
                mixed = format!(
                    "; the model job: {} of the models {:?} by {:?}, textures of model {}, the \
                     ground point at {height:?}",
                    request.count, request.models, request.weights, request.textures
                );
                shape = Some(made);
            }
            let groups = bone_groups(&exe.image, exe.base)
                .ok_or_else(|| wrong(&"the executable has no bone groups"))?;
            let scale = match roams {
                true => model_scale(&stages, number).unwrap_or(stage.scale),
                false => stage.scale,
            };
            let table = format!(
                "the creatures' table at stage {number} (level {level}): scale of the model \
                 {scale:?}, groups shown {:?} and marked {:?}{mixed}",
                stage.shown, stage.marked
            );
            (
                skeleton.with_groups(groups, stage.shown, stage.marked),
                table,
                Some(scale),
                stage.bones.clone(),
            )
        }
        None => {
            let (address, whose) = match actor.record.bone_table {
                0 => (
                    defaults
                        .ok_or("the package has no default table")?
                        .bone_table,
                    "the package's own table",
                ),
                address => (address, "the table of its record"),
            };
            // A record's table lies in the package or is one of the executable's.
            let skeleton = Skeleton::at(package.image(), PACKAGE_BASE, address)
                .or_else(|_| Skeleton::at(&exe.image, exe.base, address))
                .map_err(|err| wrong(&format!("bone table {address:#010x}: {err}")))?;
            let scale = match actor.record.model_scale {
                0 => defaults.map(|defaults| defaults.model_scale),
                address => Some(address),
            };
            let model_scale = scale.and_then(|address| package.model_scale(address));
            let bone_scales = match actor.record.bone_scales {
                0 => Vec::new(),
                address => {
                    // A bone names a record by its number and by its scale byte.
                    let count = (skeleton.bones.iter())
                        .map(|bone| usize::from(bone.number.max(bone.scale)) + 1)
                        .max()
                        .unwrap_or(0);
                    package
                        .bone_scales(address, count)
                        .ok_or_else(|| wrong(&format!("bone scales {address:#010x}")))?
                }
            };
            let table = format!(
                "{whose} at {address:#010x}, scale of the model {model_scale:?}, {} bone scales",
                bone_scales.len()
            );
            (skeleton, table, model_scale, bone_scales)
        }
    };
    let scales = Scales {
        model: model_scale,
        bones: &bone_scales,
    };
    // The actor's only angle is the one about the upright axis.
    let about = rotation([0, actor.angle, 0]);
    let pose = skeleton.pose_shaped_from(about, &animation, frame, &scales, shape.as_ref());
    let mut block = record.mesh(&file).map_err(|err| wrong(&err))?;
    if let Some(job) = &job
        && let Some([second, third]) = &job.others
    {
        let words = [&words[0][..], &words[1][..], &words[2][..]];
        job.request
            .mix(&mut block, [&second.mesh, &third.mesh], words);
    }
    place.block(&mut block);
    if !memory.sent.contains(&slot) {
        let textures = match job.and_then(|job| job.textures) {
            Some(textures) => textures,
            None => record.textures(&file).map_err(|err| wrong(&err))?,
        };
        for image in &textures {
            memory.vram.upload(&place.image(&hue::turned(image, turn)));
        }
        memory.sent.push(slot);
    }
    let line = format!(
        "model {model} (file {}) in slot {slot} of object list {}, colours turned by {turn}, at \
         {:?} angle {}, frame {frame} of animation {number} (action {}, frames {} to {} and \
         round from {}, {}), {} bones with {table}, {} of them placed{}",
        record.file,
        memory.list,
        actor.place,
        actor.angle,
        actor.action,
        action.first,
        action.end,
        action.loop_start,
        if resident { "resident" } else { "from its set" },
        skeleton.bones.len(),
        pose.iter().flatten().count(),
        if runner.hidden { ", hidden" } else { "" }
    );
    let mut animations = BTreeMap::from([(number, animation)]);
    if whole {
        for action in &head.actions {
            let Some(number) = action.animation else {
                continue;
            };
            let resident = record.animations.get(number).is_some_and(|a| a.resident);
            if (resident || set.contains(&number))
                && let Entry::Vacant(kept) = animations.entry(number)
            {
                kept.insert(record.animation(&file, number).map_err(|err| wrong(&err))?);
            }
        }
    }
    // A model that a script of the room reads animations for: what the loader can bring
    // later is kept beside what is in memory now, with the sets of the model.
    // The same shelf serves an opponent of a fight of the room, whose moves read theirs.
    let (mut shelf, mut sets, mut brought) = (BTreeMap::new(), Vec::new(), Vec::new());
    if reads || fights {
        for number in 0..record.animations.len() {
            if !animations.contains_key(&number) {
                let animation = record.animation(&file, number).map_err(|err| wrong(&err))?;
                shelf.insert(number, animation);
            }
        }
    }
    if reads {
        for number in 0..record.sets.len() {
            let entries = record.set(&file, number).map_err(|err| wrong(&err))?;
            sets.push(entries.iter().map(|entry| entry.animation).collect());
        }
        let stays = |number: &usize| record.animations[*number].resident;
        brought = (set.iter().copied())
            .filter(|number| animations.contains_key(number) && !stays(number))
            .collect();
    }
    // The spawner keeps the meshes its record lists, as the loader has left them.
    let kept = Kept::of(&block, &package.actor_kept(&actor.record));
    let mut standing = Standing {
        block,
        memory: 0,
        skeleton,
        pose,
        model_scale,
        bone_scales,
        place: actor.place,
        angle: actor.angle,
        animations,
        actions: head.actions.clone(),
        points,
        shape,
        shelf,
        sets,
        brought,
        runner,
        mover: Mover::new(actor.place, actor.angle),
        arc: arc.clone(),
        ground: None,
        game_frame: 0,
        name: package.actor_name(&actor.record).unwrap_or_default(),
        mirrored: actor.record.mirror != 0,
        slot,
        moves: package.actor_moves(&actor.record),
        kept,
        roams: None,
        mate: None,
        carried: None,
        table: actor.record.word,
    };
    if actor.flags & 2 != 0 {
        standing.mover.flags |= mover::flag::CARRIED;
    }
    if actor.flags & 8 == 0 {
        standing.mover.flags |= mover::flag::WALKS;
    }
    let line = match (creature_move, actor.creature) {
        (Some(creature_move), Some(_)) => {
            let read = |animation| {
                record
                    .animation(&file, animation)
                    .map_err(|err| wrong(&err))
            };
            match given(&mut standing, exe, creature_move, read) {
                Ok(()) => format!("{line}; given move {}", creature_move.number),
                Err(why) => format!("{line}; not given move {}: {why}", creature_move.number),
            }
        }
        _ => line,
    };
    Ok((line, standing))
}
