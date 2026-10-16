//! Reader for a raw PS1 disc image (single MODE2/2352 track) and its ISO9660 filesystem.
//!
//! Nothing here contains game data. Every function reads from the user's own disc image.

mod archive;
mod config;

pub use archive::{ARCHIVE_PATH, Archive, EXE_PATH, Executable, FileEntry, TABLE_ENTRIES};
pub use config::{
    ConfigError, Controls, LOCAL_CONFIG_NAME, LocalConfig, Movement, with_disc_image,
};

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes per raw sector in a MODE2/2352 image.
pub const RAW_SECTOR: u64 = 2352;
/// Offset of the user data inside a MODE2 Form1 sector (12 sync + 4 header + 8 subheader).
pub const FORM1_DATA_OFFSET: u64 = 24;
/// User data bytes in a MODE2 Form1 sector.
pub const FORM1_DATA_SIZE: usize = 2048;

/// A raw MODE2/2352 disc image opened for sector reads.
pub struct DiscImage<R> {
    inner: R,
}

impl DiscImage<File> {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(File::open(path)?))
    }
}

impl<R: Read + Seek> DiscImage<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// Reads the 2048 user-data bytes of one Form1 sector.
    pub fn read_sector(&mut self, lba: u32) -> io::Result<[u8; FORM1_DATA_SIZE]> {
        let mut buf = [0u8; FORM1_DATA_SIZE];
        self.inner
            .seek(SeekFrom::Start(lba as u64 * RAW_SECTOR + FORM1_DATA_OFFSET))?;
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Reads one sector as it lies in the image: sync pattern, header, subheader and all.
    /// Movies and sound streams need the subheader to tell their sectors apart.
    pub fn read_raw_sector(&mut self, lba: u32) -> io::Result<[u8; RAW_SECTOR as usize]> {
        let mut buf = [0u8; RAW_SECTOR as usize];
        self.inner.seek(SeekFrom::Start(lba as u64 * RAW_SECTOR))?;
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Reads `size` bytes of Form1 data starting at sector `lba`.
    pub fn read_bytes(&mut self, lba: u32, size: usize) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size);
        let mut sector = lba;
        while out.len() < size {
            let data = self.read_sector(sector)?;
            let take = (size - out.len()).min(FORM1_DATA_SIZE);
            out.extend_from_slice(&data[..take]);
            sector += 1;
        }
        Ok(out)
    }

    /// Lists every file in the ISO9660 filesystem, depth first, with `/`-separated paths.
    pub fn list_files(&mut self) -> io::Result<Vec<IsoFile>> {
        let pvd = self.read_sector(16)?;
        if &pvd[1..6] != b"CD001" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no ISO9660 primary volume descriptor at sector 16",
            ));
        }
        let root = DirRecord::parse(&pvd[156..190]).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "bad ISO9660 root directory record",
            )
        })?;
        let mut files = Vec::new();
        self.walk_dir(root.lba, root.size, "", &mut files)?;
        Ok(files)
    }

    fn walk_dir(
        &mut self,
        lba: u32,
        size: u32,
        prefix: &str,
        out: &mut Vec<IsoFile>,
    ) -> io::Result<()> {
        let data = self.read_bytes(lba, size as usize)?;
        let mut pos = 0;
        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                // Records never straddle a sector; a zero length means "skip to the next one".
                pos = (pos / FORM1_DATA_SIZE + 1) * FORM1_DATA_SIZE;
                continue;
            }
            let Some(rec) = data.get(pos..pos + len).and_then(DirRecord::parse) else {
                break;
            };
            pos += len;
            // Identifiers 0x00 and 0x01 are the directory itself and its parent.
            if rec.name == "\0" || rec.name == "\u{1}" {
                continue;
            }
            let path = format!("{prefix}/{}", rec.name);
            if rec.is_dir {
                self.walk_dir(rec.lba, rec.size, &path, out)?;
            } else {
                out.push(IsoFile {
                    path,
                    lba: rec.lba,
                    size: rec.size,
                });
            }
        }
        Ok(())
    }
}

/// One file in the disc's ISO9660 filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoFile {
    /// Path from the root, e.g. `/DATA/DATA.001`. The `;1` version suffix is stripped.
    pub path: String,
    pub lba: u32,
    pub size: u32,
}

struct DirRecord {
    lba: u32,
    size: u32,
    is_dir: bool,
    name: String,
}

impl DirRecord {
    fn parse(rec: &[u8]) -> Option<Self> {
        if rec.len() < 34 {
            return None;
        }
        let name_len = rec[32] as usize;
        let raw = rec.get(33..33 + name_len)?;
        let name = String::from_utf8_lossy(raw);
        let name = name.split(';').next().unwrap_or_default().to_string();
        Some(Self {
            lba: u32::from_le_bytes(rec[2..6].try_into().ok()?),
            size: u32::from_le_bytes(rec[10..14].try_into().ok()?),
            is_dir: rec[25] & 2 != 0,
            name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn dir_record(name: &[u8], lba: u32, size: u32, is_dir: bool) -> Vec<u8> {
        let mut len = 33 + name.len();
        len += len % 2;
        let mut rec = vec![0u8; len];
        rec[0] = len as u8;
        rec[2..6].copy_from_slice(&lba.to_le_bytes());
        rec[10..14].copy_from_slice(&size.to_le_bytes());
        rec[25] = if is_dir { 2 } else { 0 };
        rec[32] = name.len() as u8;
        rec[33..33 + name.len()].copy_from_slice(name);
        rec
    }

    fn put(image: &mut [u8], lba: u32, data: &[u8]) {
        let start = (lba as u64 * RAW_SECTOR + FORM1_DATA_OFFSET) as usize;
        image[start..start + data.len()].copy_from_slice(data);
    }

    /// A synthetic image: root at sector 18 with one file and one directory at sector 19.
    fn synthetic_image() -> Vec<u8> {
        let mut image = vec![0u8; 21 * RAW_SECTOR as usize];

        let mut pvd = vec![0u8; FORM1_DATA_SIZE];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        let root = dir_record(b"\0", 18, 2048, true);
        pvd[156..156 + root.len()].copy_from_slice(&root);
        put(&mut image, 16, &pvd);

        let mut root_dir = Vec::new();
        root_dir.extend(dir_record(b"\0", 18, 2048, true));
        root_dir.extend(dir_record(b"\x01", 18, 2048, true));
        root_dir.extend(dir_record(b"BOOT.EXE;1", 20, 5, false));
        root_dir.extend(dir_record(b"DATA", 19, 2048, true));
        put(&mut image, 18, &root_dir);

        let mut data_dir = Vec::new();
        data_dir.extend(dir_record(b"\0", 19, 2048, true));
        data_dir.extend(dir_record(b"\x01", 18, 2048, true));
        data_dir.extend(dir_record(b"PACK.001;1", 20, 5, false));
        put(&mut image, 19, &data_dir);

        put(&mut image, 20, b"hello");
        image
    }

    #[test]
    fn lists_nested_files_and_strips_version() {
        let mut disc = DiscImage::new(Cursor::new(synthetic_image()));
        let files = disc.list_files().unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["/BOOT.EXE", "/DATA/PACK.001"]);
        assert_eq!(files[0].lba, 20);
        assert_eq!(files[0].size, 5);
    }

    #[test]
    fn reads_file_bytes_from_form1_data_area() {
        let mut disc = DiscImage::new(Cursor::new(synthetic_image()));
        assert_eq!(disc.read_bytes(20, 5).unwrap(), b"hello");
    }

    #[test]
    fn reads_a_sector_with_its_subheader() {
        let mut image = synthetic_image();
        let start = 20 * RAW_SECTOR as usize;
        image[start + 16..start + 20].copy_from_slice(&[1, 1, 0x48, 0]);
        let mut disc = DiscImage::new(Cursor::new(image));
        let raw = disc.read_raw_sector(20).unwrap();
        assert_eq!(raw[16..20], [1, 1, 0x48, 0]);
        assert_eq!(&raw[24..29], b"hello");
        assert!(
            disc.read_raw_sector(21).is_err(),
            "past the end of the image"
        );
    }

    #[test]
    fn rejects_image_without_volume_descriptor() {
        let mut disc = DiscImage::new(Cursor::new(vec![0u8; 21 * RAW_SECTOR as usize]));
        assert!(disc.list_files().is_err());
    }
}
