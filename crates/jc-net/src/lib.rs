
pub mod lan;
pub mod link;
pub mod online;
pub mod session;
pub mod upnp;
pub mod wire;

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Seek};
use std::time::{SystemTime, UNIX_EPOCH};

use jc_disc::{Archive, DiscImage, Executable};
use jc_formats::battle::duel::Hash;

/// The port a host listens on, unless the player sets another.
pub const PORT: u16 = 47820;
/// The port of "who hosts?" on the local network.
pub const LAN_PORT: u16 = 47821;
/// The build's version and the commit it was built from (`build.rs`), which both
/// players must share.
pub const BUILD: &str = env!("JC_BUILD");
/// The form of the messages; a change of [`wire`] counts it up.
pub const PROTOCOL: u16 = 1;
pub const RULES_FILES: std::ops::RangeInclusive<usize> = 29..=36;

/// The hash of the rules both machines play by: the executable, which holds every table
/// of the fight, and the files of [`RULES_FILES`] (FNV-1a, as the duel's state hash).
pub fn rules_hash(executable: &[u8], files: &[Vec<u8>]) -> u64 {
    let mut hash = Hash::new();
    hash.word(executable.len() as u32);
    hash.bytes(executable);
    for file in files {
        hash.word(file.len() as u32);
        hash.bytes(file);
    }
    hash.0
}

/// [`rules_hash`] of a disc.
pub fn read_rules_hash<R: Read + Seek>(disc: &mut DiscImage<R>) -> io::Result<u64> {
    let exe = Executable::read(disc)?;
    let archive = Archive::open(disc)?;
    let files = RULES_FILES
        .map(|file| archive.read(disc, file))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(rules_hash(&exe.image, &files))
}

/// 32 bits nobody picks: a side's half of a seed. From the process's random hash keys and
/// the clock; never from the duel's generator.
pub fn fresh_bits() -> u32 {
    let mut hasher = RandomState::new().build_hasher();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    hasher.write_u128(now);
    hasher.write_u32(std::process::id());
    let bits = hasher.finish();
    (bits ^ bits >> 32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_hash_tells_files_apart() {
        let files = vec![vec![1, 2, 3], vec![4]];
        let hash = rules_hash(&[9; 16], &files);
        assert_eq!(hash, rules_hash(&[9; 16], &files));
        assert_ne!(hash, rules_hash(&[9; 15], &files));
        // The lengths count: the same bytes cut elsewhere are other files.
        assert_ne!(hash, rules_hash(&[9; 16], &[vec![1, 2], vec![3, 4]]));
    }

    #[test]
    fn the_build_names_its_version_and_fits_a_hello() {
        assert!(BUILD.starts_with(env!("CARGO_PKG_VERSION")), "{BUILD}");
        assert!(BUILD.len() <= wire::MOST_BUILD, "{BUILD}");
    }

    #[test]
    fn fresh_bits_differ() {
        let bits: Vec<u32> = (0..8).map(|_| fresh_bits()).collect();
        assert!(bits.iter().any(|b| *b != bits[0]));
    }
}
