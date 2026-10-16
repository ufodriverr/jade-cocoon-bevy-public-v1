//! The machine-local configuration file, `jc.local.toml`.
//!
//! It holds the user's local disc and cache paths and is never committed.
//! `jc.local.example.toml` is the template.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Archive, DiscImage, Executable};

/// File name of the local configuration, looked up from the working directory upwards.
pub const LOCAL_CONFIG_NAME: &str = "jc.local.toml";

/// Paths from `jc.local.toml`. Relative paths are resolved against the file's directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConfig {
    /// The raw MODE2/2352 disc image.
    pub disc_image: PathBuf,
    /// Where converted assets are written.
    pub cache_dir: PathBuf,
    /// How the keyboard steers the player (the table `[controls]`).
    pub controls: Controls,
}

/// The table `[controls]` of `jc.local.toml`: choices of the build for a PC, not of the
/// game. A file without it gets the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    /// How the directions move the player.
    pub movement: Movement,
    /// The directions alone run, and Shift makes a walk of it; false is the other way
    /// round, as the pad's run button is.
    pub always_run: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            movement: Movement::Modern,
            always_run: true,
        }
    }
}

/// How the directions move the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Movement {
    /// The way the screen shows: up goes up the picture, left to its left, and the
    /// player turns there by itself (the game's own steering of an analog stick).
    #[default]
    Modern,
    /// The pad's buttons as the game reads them: up and down walk forward and back, the
    /// two sides turn on the spot.
    Tank,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    disc_image: PathBuf,
    #[serde(default = "default_cache_dir")]
    cache_dir: PathBuf,
    #[serde(default)]
    controls: Controls,
}

fn default_cache_dir() -> PathBuf {
    PathBuf::from("cache")
}

impl LocalConfig {
    /// Finds `jc.local.toml` in the working directory or the nearest ancestor and loads it.
    pub fn load() -> Result<Self, ConfigError> {
        let start = std::env::current_dir().map_err(ConfigError::Io)?;
        Self::load_from(&start)
    }

    /// Like [`LocalConfig::load`], searching from `start` upwards.
    pub fn load_from(start: &Path) -> Result<Self, ConfigError> {
        let path =
            Self::find_from(start).ok_or_else(|| ConfigError::NotFound(start.to_path_buf()))?;
        Self::from_file(&path)
    }

    /// The `jc.local.toml` that [`LocalConfig::load_from`] reads: the one in `start` or in
    /// its nearest ancestor that has one.
    pub fn find_from(start: &Path) -> Option<PathBuf> {
        start
            .ancestors()
            .map(|dir| dir.join(LOCAL_CONFIG_NAME))
            .find(|candidate| candidate.is_file())
    }

    /// Stores `image` as the disc image for every later start from `start`: in the file
    /// [`LocalConfig::find_from`] names, whose other lines stay, or in a new one in
    /// `start`. Answers the file it wrote.
    pub fn store_disc_image(start: &Path, image: &Path) -> io::Result<PathBuf> {
        let file = Self::find_from(start).unwrap_or_else(|| start.join(LOCAL_CONFIG_NAME));
        let text = match std::fs::read_to_string(&file) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
            Err(err) => return Err(err),
        };
        std::fs::write(&file, with_disc_image(&text, image))?;
        Ok(file)
    }

    /// What is wrong with a file as the disc image, in words for the person who picked
    /// it; `None` for an image the game can be read from: its sectors, the table of the
    /// archive and the executable.
    pub fn disc_problem(image: &Path) -> Option<String> {
        if !image.is_file() {
            return Some(format!("There is no file at {}.", image.display()));
        }
        let read = || -> io::Result<()> {
            let mut disc = DiscImage::open(image)?;
            Archive::open(&mut disc)?;
            Executable::read(&mut disc)?;
            Ok(())
        };
        read().err().map(|err| {
            format!(
                "{} is not a disc image of the game this build reads (Jade Cocoon, Europe, \
                 SLES-02201, a raw .bin of 2352 bytes a sector): {err}",
                image.display()
            )
        })
    }

    /// Loads the configuration from an explicit file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        let base = path.parent().unwrap_or(Path::new(""));
        Self::parse(&text, base)
    }

    /// Parses configuration text, resolving relative paths against `base`.
    pub fn parse(text: &str, base: &Path) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(ConfigError::Parse)?;
        Ok(Self {
            disc_image: base.join(raw.disc_image),
            cache_dir: base.join(raw.cache_dir),
            controls: raw.controls,
        })
    }
}

/// The text of a `jc.local.toml` with `image` as its disc image: the line of the key is
/// replaced where the text has one and put in front where it has none; every other line
/// stays as it is.
pub fn with_disc_image(text: &str, image: &Path) -> String {
    let path = image.display().to_string();
    // A literal string takes a Windows path as it is; one with a quote of that kind in
    // it needs the other kind, with its escapes.
    let line = match path.contains('\'') {
        false => format!("disc_image = '{path}'"),
        true => format!(
            "disc_image = \"{}\"",
            path.replace('\\', "\\\\").replace('"', "\\\"")
        ),
    };
    let is_key = |candidate: &str| {
        (candidate.trim_start().strip_prefix("disc_image"))
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    if !text.lines().any(is_key) {
        return format!("{line}\n{text}");
    }
    let mut out = String::new();
    for old in text.lines() {
        out.push_str(if is_key(old) { &line } else { old });
        out.push('\n');
    }
    out
}

#[derive(Debug)]
pub enum ConfigError {
    /// No `jc.local.toml` in the given directory or any of its ancestors.
    NotFound(PathBuf),
    Io(io::Error),
    Parse(toml::de::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(start) => write!(
                f,
                "no {LOCAL_CONFIG_NAME} in {} or above; copy jc.local.example.toml",
                start.display()
            ),
            Self::Io(err) => write!(f, "cannot read {LOCAL_CONFIG_NAME}: {err}"),
            Self::Parse(err) => write!(f, "bad {LOCAL_CONFIG_NAME}: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotFound(_) => None,
            Self::Io(err) => Some(err),
            Self::Parse(err) => Some(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_paths_against_the_config_directory() {
        let base = Path::new("base");
        let config =
            LocalConfig::parse("disc_image = 'game.bin'\ncache_dir = 'out'\n", base).unwrap();
        assert_eq!(config.disc_image, base.join("game.bin"));
        assert_eq!(config.cache_dir, base.join("out"));
        assert_eq!(config.controls, Controls::default());
    }

    #[test]
    fn reads_the_controls_and_rejects_unknown_ones() {
        let text = "disc_image = 'a.bin'
[controls]
movement = 'tank'
";
        let config = LocalConfig::parse(text, Path::new("")).unwrap();
        assert_eq!(config.controls.movement, Movement::Tank);
        assert!(config.controls.always_run);
        let text = "disc_image = 'a.bin'
[controls]
always_run = false
";
        let config = LocalConfig::parse(text, Path::new("")).unwrap();
        assert_eq!(config.controls.movement, Movement::Modern);
        assert!(!config.controls.always_run);
        let text = "disc_image = 'a.bin'
[controls]
movement = 'sideways'
";
        assert!(LocalConfig::parse(text, Path::new("")).is_err());
    }

    #[test]
    fn keeps_absolute_paths_and_defaults_the_cache() {
        let absolute = std::env::temp_dir().join("game.bin");
        let text = format!(
            "disc_image = '{}'\n",
            absolute.display()
        );
        let base = Path::new("base");
        let config = LocalConfig::parse(&text, base).unwrap();
        assert_eq!(config.disc_image, absolute);
        assert_eq!(config.cache_dir, base.join("cache"));
    }

    #[test]
    fn rejects_missing_disc_image_and_unknown_keys() {
        let base = Path::new("");
        assert!(matches!(
            LocalConfig::parse("cache_dir = 'cache'\n", base),
            Err(ConfigError::Parse(_))
        ));
        assert!(matches!(
            LocalConfig::parse("disc_image = 'a.bin'\ndisk_image = 'b.bin'\n", base),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn the_example_file_parses() {
        let example = include_str!("../../../jc.local.example.toml");
        let config = LocalConfig::parse(example, Path::new("")).unwrap();
        assert_eq!(config.cache_dir, Path::new("cache"));
    }

    #[test]
    fn a_stored_disc_image_replaces_the_old_line_and_keeps_the_rest() {
        let text = "# mine\ndisc_image = 'old.bin'\ncache_dir = 'out'\n";
        let image = Path::new(r"D:\Games\Jade Cocoon (Europe).bin");
        let stored = with_disc_image(text, image);
        assert_eq!(
            stored,
            "# mine\ndisc_image = 'D:\\Games\\Jade Cocoon (Europe).bin'\ncache_dir = \
             'out'\n"
        );
        let config = LocalConfig::parse(&stored, Path::new("base")).unwrap();
        assert_eq!(config.disc_image, image);
        assert_eq!(config.cache_dir, Path::new("base").join("out"));
    }

    #[test]
    fn a_stored_disc_image_makes_a_file_of_its_own_and_takes_a_quote() {
        let image = Path::new(r"D:\Levant's\game.bin");
        let stored = with_disc_image("", image);
        let config = LocalConfig::parse(&stored, Path::new("")).unwrap();
        assert_eq!(config.disc_image, image);
        // A file without the key gets it in front, in front of any table.
        let stored = with_disc_image("cache_dir = 'out'\n", Path::new("a.bin"));
        assert_eq!(stored, "disc_image = 'a.bin'\ncache_dir = 'out'\n");
    }

    #[test]
    fn a_file_that_is_not_there_or_not_the_disc_is_a_problem() {
        let nowhere = Path::new("jc-config-test-missing-dir").join("game.bin");
        let problem = LocalConfig::disc_problem(&nowhere).expect("a problem");
        assert!(problem.starts_with("There is no file at"), "{problem}");
        // This source file is a file, but no disc image.
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("config.rs");
        let problem = LocalConfig::disc_problem(&source).expect("a problem");
        assert!(problem.contains("is not a disc image"), "{problem}");
        assert_eq!(
            LocalConfig::find_from(Path::new("jc-config-test-missing-dir")),
            None
        );
    }

    #[test]
    fn reports_a_missing_file() {
        // A path that exists nowhere, so no ancestor holds a config either.
        let nowhere = Path::new("jc-config-test-missing-dir");
        assert!(matches!(
            LocalConfig::load_from(nowhere),
            Err(ConfigError::NotFound(_))
        ));
    }
}
