
use crate::background::{PointLight, ViewHeader};

/// One in the game's fixed point, and the bits behind the point.
pub const ONE: i32 = 4096;
const POINT: u32 = 12;
/// The base colour of a lit textured corner: the value that leaves a texel as it is.
pub const TEXEL_UNCHANGED: u8 = 128;
/// The largest value the console's lighting keeps of a light or of a colour sum.
const LIMIT: i64 = 0x7FFF;

/// The lights of a view for one model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lights {
    /// Direction of the room's light, of length [`ONE`]. The light from behind is its opposite.
    pub direction: [i32; 3],
    /// Colour of the room's light; [`ONE`] is full.
    pub colour: [i32; 3],
    /// The ambient colour in the same unit.
    pub ambient: [i32; 3],
    /// The point light picked for where the model stands.
    pub point: PointLight,
    /// The movable light, while one is set.
    pub movable: Option<Movable>,
}

/// The movable fourth light (`0x8008D610`, `0x8008D3A8`, `0x8008D380`), which the
/// effects of a fight set: on a bone that is near it, it takes the place of the light
/// from behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Movable {
    pub place: [i32; 3],
    /// Its colour at the light itself; [`ONE`] is full.
    pub colour: [i32; 3],
    /// How far it reaches; 0 is no light.
    pub range: i32,
}

impl Movable {
    /// The direction from a bone at `place` to the light and the share of its colour
    /// the bone gets: the light counts when the bone is nearer than the range on every
    /// axis and not farther than it in all, and its colour falls in a straight line
    /// with the distance, not with its square.
    fn on(&self, place: [i32; 3]) -> Option<([i32; 3], i64)> {
        let range = i64::from(self.range);
        let toward: [i64; 3] =
            std::array::from_fn(|k| i64::from(self.place[k]) - i64::from(place[k]));
        if range == 0 || toward.iter().any(|part| part.abs() >= range) {
            return None;
        }
        // A bone at the light itself has no direction to it (the game divides by 0).
        let (direction, length) = unit(toward).unwrap_or(([0; 3], 0));
        (length <= range).then(|| (direction, (range - length) * i64::from(ONE) / range))
    }
}

/// The floor of a square root, as the game's routine gives it.
fn root(value: i64) -> i64 {
    let mut root = (value as f64).sqrt() as i64;
    while root * root > value {
        root -= 1;
    }
    while (root + 1) * (root + 1) <= value {
        root += 1;
    }
    root
}

/// The unit direction along `toward` and its length. `None` for no length at all.
fn unit(toward: [i64; 3]) -> Option<([i32; 3], i64)> {
    let length = root(toward.iter().map(|part| part * part).sum());
    (length != 0).then(|| {
        (
            toward.map(|part| (part * i64::from(ONE) / length) as i32),
            length,
        )
    })
}

impl Lights {
    /// The lights of a view for a model that stands at `position`: the room's light, the
    /// ambient colour, and the point light that is nearest on the level among the records up
    /// to the first without a range (record 0 when there is none).
    pub fn of_view(header: &ViewHeader, position: [i32; 3]) -> Self {
        let level = |light: &PointLight| {
            let x = i64::from(light.position[0]) - i64::from(position[0]);
            let z = i64::from(light.position[2]) - i64::from(position[2]);
            x * x + z * z
        };
        let point = (header.point_lights.iter())
            .take_while(|light| light.range != 0)
            .enumerate()
            // Of two lights at one distance the game keeps the first.
            .min_by_key(|(index, light)| (level(light), *index))
            .map_or(header.point_lights[0], |(_, light)| *light);
        let direction =
            unit(header.light_direction.map(i64::from)).map_or([0; 3], |(unit, _)| unit);
        Self {
            direction,
            colour: header.light_color.map(i32::from),
            ambient: header.color.map(|part| i32::from(part) << 4),
            point,
            movable: None,
        }
    }

    /// These lights with the movable light as it is set now.
    pub fn with_movable(mut self, movable: Option<Movable>) -> Self {
        self.movable = movable;
        self
    }

    /// The lights for a bone that stands at `place` (in the room) and whose mesh object is
    /// turned by the rows of `matrix`.
    pub fn on_bone(&self, place: [i32; 3], matrix: &[[i32; 3]; 3]) -> BoneLight {
        let toward: [i64; 3] =
            std::array::from_fn(|k| i64::from(self.point.position[k]) - i64::from(place[k]));
        let range = i64::from(self.point.range);
        // The point light fades with the square of what is left of its range.
        let (to_point, point_colour) = match unit(toward) {
            Some((direction, length)) if range > 0 && length <= range => {
                let left = range - length;
                let share = left * i64::from(ONE) / range * left / range;
                let colour = self
                    .point
                    .color
                    .map(|part| ((i64::from(part) * share) >> POINT) as i32);
                (direction, colour)
            }
            _ => ([0; 3], [0; 3]),
        };
        // The light from behind, or the movable light in its place.
        let near = self
            .movable
            .and_then(|light| Some((light, light.on(place)?)));
        let (third, third_colour) = match near {
            Some((light, (direction, share))) => (
                direction,
                (light.colour).map(|part| ((i64::from(part) * share) >> POINT) as i32),
            ),
            None => (
                self.direction.map(|part| -part),
                self.colour.map(|part| part / 4),
            ),
        };
        let world = [self.direction, to_point, third];
        // The three directions as rows, times the bone's matrix: the directions in the space
        // of the bone's mesh object.
        let rows = world.map(|row| {
            std::array::from_fn(|column| {
                let sum: i64 = (0..3)
                    .map(|k| i64::from(row[k]) * i64::from(matrix[k][column]))
                    .sum();
                (sum >> POINT) as i32
            })
        });
        BoneLight {
            rows,
            colours: [self.colour, point_colour, third_colour],
            ambient: self.ambient,
            movable: near.map(|(light, (_, share))| (light.colour, share as i32)),
        }
    }
}

/// The three lights in the space of one bone's mesh object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoneLight {
    /// Direction of each light, of length [`ONE`].
    pub rows: [[i32; 3]; 3],
    /// Colour of each light.
    pub colours: [[i32; 3]; 3],
    pub ambient: [i32; 3],
    /// The movable light when it stands in for the third light: its colour at the
    /// light itself and the share of it the bone gets. The screen's brightness dims
    /// the share (`Fade::lights`).
    pub movable: Option<([i32; 3], i32)>,
}

impl BoneLight {
    /// How much light falls on a normal, for red, green and blue: [`ONE`] leaves the base
    /// colour as it is.
    pub fn light(&self, normal: [i16; 3]) -> [i32; 3] {
        let lights = self.rows.map(|row| {
            let sum: i64 = (0..3)
                .map(|k| i64::from(row[k]) * i64::from(normal[k]))
                .sum();
            (sum >> POINT).clamp(0, LIMIT)
        });
        std::array::from_fn(|channel| {
            let sum: i64 = (0..3)
                .map(|light| i64::from(self.colours[light][channel]) * lights[light])
                .sum();
            (((i64::from(self.ambient[channel]) << POINT) + sum) >> POINT).clamp(0, LIMIT) as i32
        })
    }

    /// The colour of a lit corner: the light on its normal times `base`, faded towards
    /// black by `fade` of [`ONE`].
    pub fn colour(&self, normal: [i16; 3], base: [u8; 3], fade: i32) -> [u8; 3] {
        let light = self.light(normal);
        std::array::from_fn(|channel| {
            faded(
                (i64::from(base[channel]) << 4) * i64::from(light[channel]),
                fade,
            )
        })
    }
}

/// A colour value with 16 bits behind the point, mixed towards black and cut to a byte.
fn faded(value: i64, fade: i32) -> u8 {
    let to_black = (-value >> POINT).clamp(-0x8000, LIMIT);
    let mixed = (to_black * i64::from(fade) + value) >> POINT;
    (mixed >> 4).clamp(0, 255) as u8
}

/// The colour of a corner of an object that is not lit: the stored colour, faded towards
/// black by `fade` of [`ONE`].
pub fn unlit(base: [u8; 3], fade: i32) -> [u8; 3] {
    base.map(|part| faded(i64::from(part) << 16, fade))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::POINT_LIGHTS;

    const IDENTITY: [[i32; 3]; 3] = [[ONE, 0, 0], [0, ONE, 0], [0, 0, ONE]];

    fn header() -> ViewHeader {
        ViewHeader {
            player_position: [0; 3],
            player_angles: [0; 3],
            camera: [0; 3],
            look_at: [0; 3],
            roll: 0,
            projection_distance: 320,
            color: [64, 32, 0],
            light_direction: [0, -200, 0],
            light_color: [4096, 2048, 4096],
            point_lights: [PointLight::default(); POINT_LIGHTS],
        }
    }

    #[test]
    fn room_light_and_the_light_from_behind() {
        let lights = Lights::of_view(&header(), [0; 3]);
        assert_eq!(lights.direction, [0, -ONE, 0]);
        assert_eq!(lights.ambient, [1024, 512, 0]);
        let bone = lights.on_bone([0; 3], &IDENTITY);
        // A normal that faces the light: ambient plus the light's colour.
        assert_eq!(bone.light([0, -4096, 0]), [1024 + 4096, 512 + 2048, 4096]);
        // One that faces away gets a quarter of it, from behind.
        assert_eq!(bone.light([0, 4096, 0]), [1024 + 1024, 512 + 512, 1024]);
        // Across the light there is only the ambient colour.
        assert_eq!(bone.light([4096, 0, 0]), [1024, 512, 0]);
        // Full light leaves 128 as it is; more brightens up to the limit of a byte.
        assert_eq!(bone.colour([0, -4096, 0], [128; 3], 0), [160, 80, 128]);
        assert_eq!(bone.colour([0, -4096, 0], [250; 3], 0), [255, 156, 250]);
    }

    #[test]
    fn directions_turn_with_the_bone() {
        let lights = Lights::of_view(&header(), [0; 3]);
        // A bone turned a quarter about z: its own x axis points down the room's y.
        let turned = [[0, -ONE, 0], [ONE, 0, 0], [0, 0, ONE]];
        let bone = lights.on_bone([0; 3], &turned);
        assert_eq!(bone.rows[0], [-ONE, 0, 0]);
        assert_eq!(bone.rows[2], [ONE, 0, 0]);
    }

    #[test]
    fn point_light_is_the_nearest_and_fades_with_the_square() {
        let mut header = header();
        header.light_color = [0; 3];
        header.color = [0; 3];
        header.point_lights[0] = PointLight {
            position: [5000, 0, 0],
            color: [4096, 4096, 4096],
            range: 1000,
        };
        header.point_lights[1] = PointLight {
            position: [500, -9000, 0],
            color: [4096, 0, 2048],
            range: 1000,
        };
        // A record behind one without a range is not looked at.
        header.point_lights[3] = PointLight {
            position: [0, 0, 0],
            color: [1, 1, 1],
            range: 50,
        };
        // Height does not count for the pick: record 1 is nearer on the level.
        let lights = Lights::of_view(&header, [0, 0, 0]);
        assert_eq!(lights.point, header.point_lights[1]);
        // Half the range away: a quarter of the colour, from the light's side.
        let bone = lights.on_bone([0, -9000, 0], &IDENTITY);
        assert_eq!(bone.rows[1], [ONE, 0, 0]);
        assert_eq!(bone.colours[1], [1024, 0, 512]);
        assert_eq!(bone.light([4096, 0, 0]), [1024, 0, 512]);
        // Out of range there is nothing.
        let far = lights.on_bone([0, 0, 0], &IDENTITY);
        assert_eq!(far.colours[1], [0; 3]);
        // Without any light in use the pick is record 0.
        header.point_lights[0].range = 0;
        assert_eq!(
            Lights::of_view(&header, [0; 3]).point,
            header.point_lights[0]
        );
    }

    #[test]
    fn the_movable_light_takes_the_place_of_the_light_from_behind() {
        let light = Movable {
            place: [300, -400, 0],
            colour: [7424, 7424, 4096],
            range: 1500,
        };
        let lights = Lights::of_view(&header(), [0; 3]).with_movable(Some(light));
        // 500 away of 1500: two thirds of the colour, from the light's side.
        let bone = lights.on_bone([0; 3], &IDENTITY);
        assert_eq!(bone.rows[2], [2457, -3276, 0]);
        assert_eq!(bone.colours[2], [4948, 4948, 2730]);
        assert_eq!(bone.movable, Some(([7424, 7424, 4096], 2730)));
        // The room's light and the ambient colour stay as they are.
        assert_eq!(bone.rows[0], [0, -ONE, 0]);
        assert_eq!(bone.colours[0], [4096, 2048, 4096]);
        // As far as the range on one axis: the light from behind is back.
        let behind = Lights::of_view(&header(), [0; 3]).on_bone([0; 3], &IDENTITY);
        assert_eq!(
            lights.on_bone([300, 1100, 0], &IDENTITY).colours,
            behind.colours
        );
        assert_eq!(lights.on_bone([300, 1100, 0], &IDENTITY).movable, None);
        // Nearer than the range on every axis and farther in all: the same.
        assert_eq!(
            lights.on_bone([-800, 700, 1000], &IDENTITY).rows,
            behind.rows
        );
        // A range of 0 is no light.
        let off = Movable { range: 0, ..light };
        let lights = Lights::of_view(&header(), [0; 3]).with_movable(Some(off));
        assert_eq!(lights.on_bone([300, -400, 0], &IDENTITY), behind);
    }

    #[test]
    fn fade_goes_to_black() {
        assert_eq!(unlit([128, 127, 0], 0), [128, 127, 0]);
        assert_eq!(unlit([128, 128, 128], ONE / 2), [64, 64, 64]);
        assert_eq!(unlit([128, 255, 3], ONE), [0, 0, 0]);
    }
}
