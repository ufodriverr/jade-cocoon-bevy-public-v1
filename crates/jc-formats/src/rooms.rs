
use std::fmt;

/// Archive file that holds the field engine and, inside it, the room table.
pub const FIELD_ENGINE_FILE: usize = 27;
/// RAM address of the first byte of [`FIELD_ENGINE_FILE`].
pub const FIELD_ENGINE_BASE: u32 = 0x800A_0530;
/// RAM address of the room table: one pointer per area.
pub const ROOM_TABLE: u32 = 0x800B_07D8;
/// Areas in the room table.
pub const AREA_COUNT: usize = 6;
/// Bytes of one room record.
const RECORD_SIZE: usize = 16;

/// One room record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomRecord {
    /// Archive file of the scene package.
    pub package_file: u16,
    /// Archive file of the backgrounds.
    pub background_file: u16,
    /// Number of the physical room inside its place. Story variants of one room share it.
    pub slot: i16,
    /// The global room number, `None` for the rooms that have none.
    pub number: Option<u16>,
    /// Music number; negative leaves the music alone.
    pub music: i16,
    /// Non-zero forces the slower kind of room change.
    pub flag: i16,
    /// RAM address of the scene package's entry function, 0 when it has none.
    pub entry: u32,
}

/// Where a record sits in the table. `room` is the record's index in its place, which is
/// not its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomLocation {
    pub area: usize,
    pub place: usize,
    pub room: usize,
}

/// Why the table could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomTableError {
    /// A pointer or a list leads outside the image. Holds the RAM address that was wanted.
    OutOfImage(u32),
}

impl fmt::Display for RoomTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfImage(address) => {
                write!(f, "room table leads outside the image, to {address:#010x}")
            }
        }
    }
}

impl std::error::Error for RoomTableError {}

/// The whole room table: areas, their places, the room records of each place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomTable {
    areas: Vec<Vec<Vec<RoomRecord>>>,
}

pub fn load_address(file: usize) -> u32 {
    match file {
        23..=28 => FIELD_ENGINE_BASE,
        30..=35 => 0x800A_4018,
        36 | 41..=43 | 46 | 48 | 51 | 52 => 0x800B_6890,
        44 | 45 | 49 | 53 => 0x800B_6ED0,
        _ => crate::scene::PACKAGE_BASE,
    }
}

/// A memory image and the RAM address of its first byte.
pub(crate) struct Image<'a> {
    pub(crate) data: &'a [u8],
    pub(crate) base: u32,
}

impl Image<'_> {
    fn bytes<const N: usize>(&self, address: u32) -> Result<[u8; N], RoomTableError> {
        let err = RoomTableError::OutOfImage(address);
        let start = address.checked_sub(self.base).ok_or(err)? as usize;
        let bytes = self.data.get(start..).and_then(|rest| rest.get(..N));
        Ok(bytes.ok_or(err)?.try_into().expect("slice of N bytes"))
    }

    pub(crate) fn u8(&self, address: u32) -> Result<u8, RoomTableError> {
        Ok(u8::from_le_bytes(self.bytes(address)?))
    }

    pub(crate) fn u32(&self, address: u32) -> Result<u32, RoomTableError> {
        Ok(u32::from_le_bytes(self.bytes(address)?))
    }

    pub(crate) fn i32(&self, address: u32) -> Result<i32, RoomTableError> {
        Ok(i32::from_le_bytes(self.bytes(address)?))
    }

    pub(crate) fn u16(&self, address: u32) -> Result<u16, RoomTableError> {
        Ok(u16::from_le_bytes(self.bytes(address)?))
    }

    pub(crate) fn i16(&self, address: u32) -> Result<i16, RoomTableError> {
        Ok(i16::from_le_bytes(self.bytes(address)?))
    }
}

/// The address `index` steps of `size` bytes after `start`.
pub(crate) fn step(start: u32, index: usize, size: usize) -> Result<u32, RoomTableError> {
    u32::try_from(index * size)
        .ok()
        .and_then(|offset| start.checked_add(offset))
        .ok_or(RoomTableError::OutOfImage(start))
}

impl RoomTable {
    /// Reads the table from the field engine file as it is on the disc.
    pub fn from_field_engine(file: &[u8]) -> Result<Self, RoomTableError> {
        Self::parse(file, FIELD_ENGINE_BASE, ROOM_TABLE, AREA_COUNT)
    }

    /// Reads a table of `areas` area pointers at RAM address `table` from an image whose
    /// first byte is at RAM address `base`.
    ///
    /// An area pointer leads to a zero-terminated list of place pointers. A place pointer
    /// leads to 16-byte room records; the list ends at a record whose first field is -1.
    pub fn parse(data: &[u8], base: u32, table: u32, areas: usize) -> Result<Self, RoomTableError> {
        let image = Image { data, base };
        let mut out = Vec::with_capacity(areas);
        for area in 0..areas {
            let place_list = image.u32(step(table, area, 4)?)?;
            let mut places = Vec::new();
            loop {
                let records = image.u32(step(place_list, places.len(), 4)?)?;
                if records == 0 {
                    break;
                }
                let mut rooms = Vec::new();
                loop {
                    let at = step(records, rooms.len(), RECORD_SIZE)?;
                    let package_file = image.i16(at)?;
                    if package_file == -1 {
                        break;
                    }
                    rooms.push(RoomRecord {
                        package_file: package_file as u16,
                        background_file: image.i16(at + 2)? as u16,
                        slot: image.i16(at + 4)?,
                        number: u16::try_from(image.i16(at + 6)?).ok(),
                        music: image.i16(at + 8)?,
                        flag: image.i16(at + 10)?,
                        entry: image.u32(at + 12)?,
                    });
                }
                places.push(rooms);
            }
            out.push(places);
        }
        Ok(Self { areas: out })
    }

    /// Number of areas.
    pub fn area_count(&self) -> usize {
        self.areas.len()
    }

    /// Number of places of an area, 0 for an area that does not exist.
    pub fn place_count(&self, area: usize) -> usize {
        self.areas.get(area).map_or(0, Vec::len)
    }

    /// The room records of a place, empty when the place does not exist.
    pub fn rooms(&self, area: usize, place: usize) -> &[RoomRecord] {
        self.areas
            .get(area)
            .and_then(|places| places.get(place))
            .map_or(&[], Vec::as_slice)
    }

    /// Every record with its location, in table order.
    pub fn iter(&self) -> impl Iterator<Item = (RoomLocation, &RoomRecord)> {
        self.areas.iter().enumerate().flat_map(|(area, places)| {
            places.iter().enumerate().flat_map(move |(place, rooms)| {
                rooms
                    .iter()
                    .enumerate()
                    .map(move |(room, record)| (RoomLocation { area, place, room }, record))
            })
        })
    }

    /// The record with a room number. Where two areas hold the same records the first one
    /// wins, as in the game's own lookup.
    pub fn by_number(&self, number: u16) -> Option<(RoomLocation, &RoomRecord)> {
        self.iter()
            .find(|(_, record)| record.number == Some(number))
    }

    /// Turns a slot of a place into a record, the way the game does on a room change that
    /// was not made by room number.
    ///
    /// `variant` is the variant table's value for (area, place, slot): `None` when it has no
    /// entry or the entry is -1. Without one the first record of the slot is taken; with one
    /// the first record of the slot that also has that room number.
    pub fn by_slot(
        &self,
        area: usize,
        place: usize,
        slot: i16,
        variant: Option<u16>,
    ) -> Option<(RoomLocation, &RoomRecord)> {
        self.rooms(area, place)
            .iter()
            .enumerate()
            .find(|(_, record)| {
                record.slot == slot && (variant.is_none() || record.number == variant)
            })
            .map(|(room, record)| (RoomLocation { area, place, room }, record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u32 = 0x8000_1000;

    /// `(package, background, slot, number, music, flag, entry)`
    type Fields = (i16, i16, i16, i16, i16, i16, u32);

    /// Builds an image: the area pointers first, then the place lists, then the records.
    /// An area given as `Err(a)` reuses the place list of area `a`.
    fn build(areas: &[Result<Vec<Vec<Fields>>, usize>]) -> Vec<u8> {
        let mut data = vec![0u8; areas.len() * 4];
        let mut list_address = vec![0u32; areas.len()];
        for (index, area) in areas.iter().enumerate() {
            let places = match area {
                Ok(places) => places,
                Err(other) => {
                    list_address[index] = list_address[*other];
                    continue;
                }
            };
            let mut pointers = Vec::new();
            for rooms in places {
                pointers.push(BASE + data.len() as u32);
                for &(package, background, slot, number, music, flag, entry) in rooms {
                    for field in [package, background, slot, number, music, flag] {
                        data.extend_from_slice(&field.to_le_bytes());
                    }
                    data.extend_from_slice(&entry.to_le_bytes());
                }
                // The end record: only its first field is read.
                data.extend_from_slice(&(-1i16).to_le_bytes());
                data.extend_from_slice(&[0xEE; 2]);
            }
            list_address[index] = BASE + data.len() as u32;
            for pointer in pointers {
                data.extend_from_slice(&pointer.to_le_bytes());
            }
            data.extend_from_slice(&0u32.to_le_bytes());
        }
        for (index, address) in list_address.iter().enumerate() {
            data[index * 4..index * 4 + 4].copy_from_slice(&address.to_le_bytes());
        }
        data
    }

    fn sample() -> Vec<u8> {
        build(&[
            Ok(vec![
                vec![(200, 500, 0, 7, 13, 0, 0x8000_2000)],
                vec![
                    (201, 501, 0, 8, -1, 1, 0),
                    (202, 501, 0, 9, 14, 0, 0),
                    (203, 502, 1, -1, 14, 0, 0),
                    (204, 502, 1, 10, 14, 0, 0),
                ],
            ]),
            Ok(vec![]),
            // The same records under another area number.
            Err(0),
        ])
    }

    fn table() -> RoomTable {
        RoomTable::parse(&sample(), BASE, BASE, 3).unwrap()
    }

    #[test]
    fn reads_areas_places_and_records() {
        let table = table();
        assert_eq!(table.area_count(), 3);
        assert_eq!(
            [0, 1, 2, 3].map(|area| table.place_count(area)),
            [2, 0, 2, 0]
        );
        assert_eq!(table.rooms(0, 1).len(), 4);
        assert!(table.rooms(0, 2).is_empty());
        assert!(table.rooms(9, 0).is_empty());
        assert_eq!(
            table.rooms(0, 0)[0],
            RoomRecord {
                package_file: 200,
                background_file: 500,
                slot: 0,
                number: Some(7),
                music: 13,
                flag: 0,
                entry: 0x8000_2000,
            }
        );
        let second = table.rooms(0, 1)[0];
        assert_eq!((second.music, second.flag, second.entry), (-1, 1, 0));
        assert_eq!(table.rooms(0, 1)[2].number, None);
        assert_eq!(table.iter().count(), 10);
    }

    #[test]
    fn finds_a_room_by_number_first_area_wins() {
        let table = table();
        let (location, record) = table.by_number(10).unwrap();
        assert_eq!(
            location,
            RoomLocation {
                area: 0,
                place: 1,
                room: 3
            }
        );
        assert_eq!(record.package_file, 204);
        assert!(table.by_number(11).is_none());
    }

    #[test]
    fn slot_lookup_follows_the_variant() {
        let table = table();
        let file = |slot, variant| {
            table
                .by_slot(2, 1, slot, variant)
                .map(|(location, record)| (location.room, record.package_file))
        };
        // No variant: the first record of the slot, even one without a room number.
        assert_eq!(file(0, None), Some((0, 201)));
        assert_eq!(file(1, None), Some((2, 203)));
        // A variant names the record by its room number.
        assert_eq!(file(0, Some(9)), Some((1, 202)));
        assert_eq!(file(1, Some(10)), Some((3, 204)));
        // A room number of another slot does not match.
        assert_eq!(file(0, Some(10)), None);
        assert_eq!(file(2, None), None);
        assert!(table.by_slot(1, 0, 0, None).is_none());
    }

    #[test]
    fn rejects_tables_that_leave_the_image() {
        let data = sample();
        // Area count too high: the fourth area pointer is the first record's bytes.
        assert!(RoomTable::parse(&data, BASE, BASE, 4).is_err());
        // A table address below the image.
        assert_eq!(
            RoomTable::parse(&data, BASE, BASE - 4, 3),
            Err(RoomTableError::OutOfImage(BASE - 4))
        );
        // A record list without its end record runs off the end of the image.
        let cut = &data[..12 + 16];
        let mut cut = cut.to_vec();
        cut[0..4].copy_from_slice(&(BASE + 28).to_le_bytes());
        cut.extend_from_slice(&(BASE + 12).to_le_bytes());
        assert_eq!(
            RoomTable::parse(&cut, BASE, BASE, 1),
            Err(RoomTableError::OutOfImage(BASE + 32))
        );
        assert!(RoomTable::from_field_engine(&[]).is_err());
    }
}
