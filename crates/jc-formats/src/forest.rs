
use crate::rooms::{self, Image, RoomTableError};

/// RAM address of the table: one pointer per area, to the lists of the area's places.
pub const FOREST_TABLE: u32 = 0x8008_3724;
/// RAM address of the word that names the one list whose numbers are views.
pub const LIST_OF_VIEWS: u32 = 0x8008_36FC;
/// Areas in the table, and the places of an area that has lists.
pub const AREAS: usize = 6;
pub const PLACES: usize = 5;
/// The number that stands for every room of a place, and the one that ends a list.
pub const EVERY_ROOM: u8 = 200;
const LIST_END: u8 = 255;

/// The list of one place.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listed {
    /// The numbers up to the end mark.
    pub numbers: Vec<u8>,
    /// The numbers are views of the place's room, not rooms.
    pub of_views: bool,
}

impl Listed {
    /// Whether the list answers yes for a room of its place seen from a view.
    pub fn holds(&self, room: i16, view: i16) -> bool {
        let asked = if self.of_views { view } else { room };
        (self.numbers.iter()).any(|&number| number == EVERY_ROOM || i16::from(number) == asked)
    }
}

/// The table of the test: for every area the lists of its places, none for an area or a
/// place without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forests {
    areas: Vec<Option<Vec<Option<Listed>>>>,
}

impl Forests {
    /// Reads the table from the loaded executable.
    pub fn from_executable(image: &[u8], base: u32) -> Result<Self, RoomTableError> {
        let image = Image { data: image, base };
        let of_views = image.u32(LIST_OF_VIEWS)?;
        let list = |at: u32| {
            let mut numbers = Vec::new();
            loop {
                let number = image.u8(rooms::step(at, numbers.len(), 1)?)?;
                if number == LIST_END {
                    break;
                }
                numbers.push(number);
            }
            Ok(Listed {
                numbers,
                of_views: at == of_views,
            })
        };
        let places = |at: u32| {
            (0..PLACES)
                .map(|place| match image.u32(rooms::step(at, place, 4)?)? {
                    0 => Ok(None),
                    at => list(at).map(Some),
                })
                .collect::<Result<Vec<_>, RoomTableError>>()
        };
        let areas = (0..AREAS)
            .map(
                |area| match image.u32(rooms::step(FOREST_TABLE, area, 4)?)? {
                    0 => Ok(None),
                    at => places(at).map(Some),
                },
            )
            .collect::<Result<_, RoomTableError>>()?;
        Ok(Self { areas })
    }

    /// The list of a place, if it has one.
    pub fn listed(&self, area: usize, place: usize) -> Option<&Listed> {
        self.areas.get(area)?.as_ref()?.get(place)?.as_ref()
    }

    /// The test: the wanted area, place, room (a slot of the place) and view.
    pub fn holds(&self, area: usize, place: usize, room: i16, view: i16) -> bool {
        (self.listed(area, place)).is_some_and(|listed| listed.holds(room, view))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_answers_for_its_rooms_for_every_room_or_for_its_views() {
        let rooms = Listed {
            numbers: vec![0, 2, 12],
            of_views: false,
        };
        assert!(rooms.holds(2, 7) && rooms.holds(12, 0));
        assert!(!rooms.holds(1, 2) && !rooms.holds(-1, 0));
        let every = Listed {
            numbers: vec![EVERY_ROOM],
            of_views: false,
        };
        assert!(every.holds(31, 0) && every.holds(-1, -1));
        let views = Listed {
            numbers: vec![5, 6],
            of_views: true,
        };
        assert!(views.holds(0, 5) && !views.holds(5, 0));
        assert!(!Listed::default().holds(0, 0));
    }

    #[test]
    fn the_table_is_read_by_area_and_place() {
        // The image starts at the word of the views' list. Area 4 has lists for its
        // places 0 (views) and 2 (every room); the other areas have none.
        let base = LIST_OF_VIEWS;
        let table = (FOREST_TABLE - base) as usize;
        let mut image = vec![0u8; table + AREAS * 4 + 32];
        let (places, views, every) = (base + 8, base + 4, base + 6);
        image[4..8].copy_from_slice(&[5, LIST_END, EVERY_ROOM, LIST_END]);
        image[0..4].copy_from_slice(&views.to_le_bytes());
        image[8..12].copy_from_slice(&views.to_le_bytes());
        image[16..20].copy_from_slice(&every.to_le_bytes());
        image[table + 16..table + 20].copy_from_slice(&places.to_le_bytes());
        let forests = Forests::from_executable(&image, base).expect("table");
        assert!(forests.holds(4, 0, 3, 5) && !forests.holds(4, 0, 5, 3));
        assert!(forests.holds(4, 2, 9, 0));
        assert!(!forests.holds(4, 1, 0, 0) && !forests.holds(0, 0, 0, 0));
        assert!(!forests.holds(9, 0, 0, 0) && !forests.holds(4, 9, 0, 0));
        // A table that leaves the image is an error.
        assert!(Forests::from_executable(&image[..table], base).is_err());
    }
}
