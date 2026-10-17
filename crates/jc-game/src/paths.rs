
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder of the saves in the game folder, where `--card-dir=DIR` does not name one.
pub const SAVES: &str = "saves";
/// The player's settings in the game folder (`prefs.rs`).
pub const SETTINGS: &str = "settings.toml";
/// The folder of the log and the crash notes in the game folder (`logging.rs`).
pub const LOGS: &str = "logs";
/// The scenes and the texts this game folder has seen, for the skip of a scene
/// (`skip.rs`): the machine's, no save keeps it.
pub const SEEN: &str = "seen.toml";

/// The game folder, found once. A phone's is the app's own folder, the working folder
/// that `android.rs` sets (its exe is Android's own).
pub fn game_folder() -> &'static Path {
    static FOLDER: OnceLock<PathBuf> = OnceLock::new();
    FOLDER.get_or_init(|| {
        let working = std::env::current_dir().unwrap_or_default();
        if cfg!(target_os = "android") {
            return working;
        }
        let exe = std::env::current_exe().ok();
        folder_of(exe.as_deref(), &working)
    })
}

/// The game folder of an exe at `exe` started from `working`.
fn folder_of(exe: Option<&Path>, working: &Path) -> PathBuf {
    let Some(beside) = exe.and_then(Path::parent) else {
        return working.to_path_buf();
    };
    let in_target = beside
        .ancestors()
        .any(|folder| folder.file_name().is_some_and(|name| name == "target"));
    match in_target {
        true => working.to_path_buf(),
        false => beside.to_path_buf(),
    }
}

/// The folder of the saves.
pub fn saves() -> PathBuf {
    game_folder().join(SAVES)
}

/// The file of the player's settings.
pub fn settings() -> PathBuf {
    game_folder().join(SETTINGS)
}

/// The folder of the logs.
pub fn logs() -> PathBuf {
    game_folder().join(LOGS)
}

/// The file of the scenes seen.
pub fn seen() -> PathBuf {
    game_folder().join(SEEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_of_the_source_keeps_the_working_folder() {
        let working = Path::new("/repo");
        let exe = Path::new("/repo/target/debug/jc-game.exe");
        assert_eq!(folder_of(Some(exe), working), working);
        let exe = Path::new("/repo/target/release/deps/jc-game.exe");
        assert_eq!(folder_of(Some(exe), working), working);
    }

    #[test]
    fn a_shipped_exe_keeps_its_own_folder() {
        let working = Path::new("/home/someone");
        let exe = Path::new("/games/Jade Cocoon/jc-game.exe");
        assert_eq!(
            folder_of(Some(exe), working),
            Path::new("/games/Jade Cocoon")
        );
        assert_eq!(folder_of(None, working), working);
    }
}
