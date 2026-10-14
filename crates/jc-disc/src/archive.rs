
use std::io::{self, Read, Seek};

use crate::{DiscImage, FORM1_DATA_SIZE, IsoFile};

/// Path of the game executable on the disc.
pub const EXE_PATH: &str = "/SLES_022.01";
/// Path of the archive on the disc.
pub const ARCHIVE_PATH: &str = "/DATA/DATA.001";

/// Size of the PS-X EXE header that precedes the loaded image in the file.
const EXE_HEADER_SIZE: u32 = 0x800;
/// Offset in the PS-X EXE header of the address the image is loaded at.
const EXE_LOAD_ADDRESS_OFFSET: usize = 0x18;
/// RAM address of the file table in the loaded executable.
const TABLE_ADDRESS: u32 = 0x800759C4;
/// Number of entries in the file table.
pub const TABLE_ENTRIES: usize = 1108;
/// Bytes per file table entry.
const ENTRY_SIZE: usize = 12;
/// The entry whose position marks the first sector of the archive.
const REBASE_ENTRY: usize = 2;
/// Sectors before LBA 0 in a minute:second:frame address (the two second pregap).
const MSF_PREGAP: u32 = 150;

/// The game executable as the console loads it: the image without the file's header.
#[derive(Debug, Clone)]
pub struct Executable {
    /// RAM address of the first byte of `image`.
    pub base: u32,
    pub image: Vec<u8>,
}

impl Executable {
    /// Reads the executable from `disc`.
    pub fn read<R: Read + Seek>(disc: &mut DiscImage<R>) -> io::Result<Self> {
        let files = disc.list_files()?;
        let exe = files
            .iter()
            .find(|f| f.path == EXE_PATH)
            .ok_or_else(|| invalid(format!("{EXE_PATH} is not on the disc")))?;
        let mut file = disc.read_bytes(exe.lba, exe.size as usize)?;
        if file.len() < EXE_HEADER_SIZE as usize || &file[..8] != b"PS-X EXE" {
            return Err(invalid(format!("{EXE_PATH} is not a PS-X executable")));
        }
        let base = u32::from_le_bytes(
            file[EXE_LOAD_ADDRESS_OFFSET..EXE_LOAD_ADDRESS_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        file.drain(..EXE_HEADER_SIZE as usize);
        Ok(Self { base, image: file })
    }
}

/// One record of the file table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileEntry {
    /// Sector the entry was authored at. Only its distance from the rebase entry matters.
    pub lba: u32,
    /// Length in 2048-byte sectors.
    pub sectors: u32,
    /// Exact length in bytes.
    pub size: u32,
}

/// The `DATA.001` archive: the file table plus where the archive sits on the disc.
#[derive(Debug, Clone)]
pub struct Archive {
    entries: Vec<FileEntry>,
    /// Authoring-time sector that corresponds to the first sector of the archive.
    rebase: u32,
    /// First sector of `DATA.001` on the disc.
    start_lba: u32,
    /// Size of `DATA.001` in bytes.
    size: u32,
}

impl Archive {
    /// Reads the file table out of the executable on `disc` and locates the archive.
    pub fn open<R: Read + Seek>(disc: &mut DiscImage<R>) -> io::Result<Self> {
        let files = disc.list_files()?;
        let find = |path: &str| {
            files
                .iter()
                .find(|f| f.path == path)
                .ok_or_else(|| invalid(format!("{path} is not on the disc")))
        };
        let exe = find(EXE_PATH)?;
        let archive = find(ARCHIVE_PATH)?;

        let header = disc.read_sector(exe.lba)?;
        if &header[..8] != b"PS-X EXE" {
            return Err(invalid(format!("{EXE_PATH} is not a PS-X executable")));
        }
        let load_address = u32::from_le_bytes(
            header[EXE_LOAD_ADDRESS_OFFSET..EXE_LOAD_ADDRESS_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let table_len = (TABLE_ENTRIES * ENTRY_SIZE) as u32;
        let table_offset = TABLE_ADDRESS
            .checked_sub(load_address)
            .map(|offset| offset + EXE_HEADER_SIZE)
            .filter(|offset| offset + table_len <= exe.size)
            .ok_or_else(|| invalid(format!("the file table is not inside {EXE_PATH}")))?;

        // Read whole sectors around the table, then cut the table out.
        let first_sector = table_offset / FORM1_DATA_SIZE as u32;
        let skip = table_offset as usize % FORM1_DATA_SIZE;
        let bytes = disc.read_bytes(exe.lba + first_sector, skip + table_len as usize)?;
        Self::from_table(&bytes[skip..], archive)
    }

    /// Builds the archive from the raw file table and the archive's ISO9660 entry.
    pub fn from_table(table: &[u8], archive: &IsoFile) -> io::Result<Self> {
        if !table.len().is_multiple_of(ENTRY_SIZE) {
            return Err(invalid("file table length is not a multiple of 12"));
        }
        let entries = table
            .as_chunks::<ENTRY_SIZE>()
            .0
            .iter()
            .enumerate()
            .map(|(index, raw)| {
                FileEntry::parse(raw)
                    .ok_or_else(|| invalid(format!("file table entry {index} has a bad position")))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let rebase = entries
            .get(REBASE_ENTRY)
            .ok_or_else(|| invalid("file table is too short"))?
            .lba;
        Ok(Self {
            entries,
            rebase,
            start_lba: archive.lba,
            size: archive.size,
        })
    }

    /// Number of entries in the file table, including those that lie outside the archive.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The table record of file `index`.
    pub fn entry(&self, index: usize) -> Option<FileEntry> {
        self.entries.get(index).copied()
    }

    /// Byte offset of file `index` inside `DATA.001`, or `None` when the index is out of
    /// range or the entry points outside the archive.
    pub fn offset(&self, index: usize) -> Option<u64> {
        let entry = self.entries.get(index)?;
        let offset = entry.lba.checked_sub(self.rebase)? as u64 * FORM1_DATA_SIZE as u64;
        (offset + entry.size as u64 <= self.size as u64).then_some(offset)
    }

    /// Disc sector that the table record `index` names, for any record at or behind the
    /// start of the archive: the files inside it, and the sound stream and the movies that
    /// lie behind it on the disc. The game finds every one of them this way.
    pub fn sector(&self, index: usize) -> Option<u32> {
        let entry = self.entries.get(index)?;
        Some(self.start_lba + entry.lba.checked_sub(self.rebase)?)
    }

    /// Reads the bytes of file `index`.
    pub fn read<R: Read + Seek>(
        &self,
        disc: &mut DiscImage<R>,
        index: usize,
    ) -> io::Result<Vec<u8>> {
        let entry = self
            .entries
            .get(index)
            .ok_or_else(|| invalid(format!("no file {index} in the file table")))?;
        let offset = self
            .offset(index)
            .ok_or_else(|| invalid(format!("file {index} is outside {ARCHIVE_PATH}")))?;
        let lba = self.start_lba + (offset / FORM1_DATA_SIZE as u64) as u32;
        disc.read_bytes(lba, entry.size as usize)
    }
}

impl FileEntry {
    /// Parses a 12-byte record: BCD minute, second, frame, an unused byte, then the sector
    /// count and the byte size as little-endian words.
    fn parse(raw: &[u8]) -> Option<Self> {
        let minute = from_bcd(raw[0])?;
        let second = from_bcd(raw[1])?;
        let frame = from_bcd(raw[2])?;
        let lba = ((minute * 60 + second) * 75 + frame).checked_sub(MSF_PREGAP)?;
        Some(Self {
            lba,
            sectors: u32::from_le_bytes(raw[4..8].try_into().ok()?),
            size: u32::from_le_bytes(raw[8..12].try_into().ok()?),
        })
    }
}

fn from_bcd(byte: u8) -> Option<u32> {
    let (tens, ones) = (byte >> 4, byte & 0xF);
    (tens < 10 && ones < 10).then_some((tens * 10 + ones) as u32)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FORM1_DATA_OFFSET, RAW_SECTOR};
    use std::io::Cursor;

    fn to_bcd(value: u32) -> u8 {
        (value / 10 * 16 + value % 10) as u8
    }

    fn entry(lba: u32, size: u32) -> [u8; ENTRY_SIZE] {
        let msf = lba + MSF_PREGAP;
        let mut raw = [0u8; ENTRY_SIZE];
        raw[0] = to_bcd(msf / 75 / 60);
        raw[1] = to_bcd(msf / 75 % 60);
        raw[2] = to_bcd(msf % 75);
        raw[4..8].copy_from_slice(&size.div_ceil(FORM1_DATA_SIZE as u32).to_le_bytes());
        raw[8..12].copy_from_slice(&size.to_le_bytes());
        raw
    }

    /// A table whose archive starts at authoring sector 392: two entries before it, three
    /// inside it and one past its end.
    fn synthetic_table() -> Vec<u8> {
        [
            entry(23, 4000),
            entry(391, 1),
            entry(392, 1),
            entry(394, 3000),
            entry(396, 5),
            entry(5000, 10),
        ]
        .concat()
    }

    /// `DATA.001` is 5 sectors long and starts at disc sector 7.
    fn synthetic_archive() -> IsoFile {
        IsoFile {
            path: ARCHIVE_PATH.into(),
            lba: 7,
            size: 5 * FORM1_DATA_SIZE as u32,
        }
    }

    #[test]
    fn decodes_bcd_positions_and_sizes() {
        let archive = Archive::from_table(&synthetic_table(), &synthetic_archive()).unwrap();
        assert_eq!(archive.len(), 6);
        assert_eq!(
            archive.entry(3),
            Some(FileEntry {
                lba: 394,
                sectors: 2,
                size: 3000
            })
        );
        // 5000 sectors is past one minute, so this exercises all three BCD fields.
        assert_eq!(archive.entry(5).unwrap().lba, 5000);
        assert_eq!(archive.entry(6), None);
    }

    #[test]
    fn offsets_are_relative_to_the_rebase_entry() {
        let archive = Archive::from_table(&synthetic_table(), &synthetic_archive()).unwrap();
        assert_eq!(archive.offset(0), None, "before the archive");
        assert_eq!(archive.offset(1), None, "before the archive");
        assert_eq!(archive.offset(2), Some(0));
        assert_eq!(archive.offset(3), Some(2 * 2048));
        assert_eq!(archive.offset(4), Some(4 * 2048));
        assert_eq!(archive.offset(5), None, "past the end of the archive");
        assert_eq!(archive.offset(6), None, "no such entry");
    }

    #[test]
    fn sectors_reach_behind_the_archive() {
        let archive = Archive::from_table(&synthetic_table(), &synthetic_archive()).unwrap();
        assert_eq!(archive.sector(0), None, "before the archive");
        assert_eq!(archive.sector(2), Some(7));
        assert_eq!(archive.sector(3), Some(9));
        assert_eq!(archive.sector(5), Some(7 + 5000 - 392));
        assert_eq!(archive.sector(6), None, "no such entry");
    }

    #[test]
    fn reads_a_file_spanning_sectors() {
        let archive = Archive::from_table(&synthetic_table(), &synthetic_archive()).unwrap();
        let content: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8).collect();

        // Entry 3 sits two sectors into the archive, so at disc sectors 9 and 10.
        let mut image = vec![0u8; 12 * RAW_SECTOR as usize];
        for (i, chunk) in content.chunks(FORM1_DATA_SIZE).enumerate() {
            let start = ((9 + i as u64) * RAW_SECTOR + FORM1_DATA_OFFSET) as usize;
            image[start..start + chunk.len()].copy_from_slice(chunk);
        }
        let mut disc = DiscImage::new(Cursor::new(image));

        assert_eq!(archive.read(&mut disc, 3).unwrap(), content);
        assert!(archive.read(&mut disc, 0).is_err());
        assert!(archive.read(&mut disc, 6).is_err());
    }

    #[test]
    fn rejects_malformed_tables() {
        let archive = synthetic_archive();
        assert!(Archive::from_table(&[0u8; 13], &archive).is_err());
        assert!(Archive::from_table(&entry(392, 1), &archive).is_err());
        let mut bad = synthetic_table();
        bad[1] = 0x7A; // not a BCD digit pair
        assert!(Archive::from_table(&bad, &archive).is_err());
    }
}
