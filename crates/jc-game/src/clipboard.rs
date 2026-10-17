//! The system's clipboard, for the room code of the lobby (`lobby.rs`): `arboard` on a
//! PC, the clipboard of Android on a phone (`android.rs`).

/// Puts text on the clipboard.
#[cfg(not(target_os = "android"))]
pub fn set(text: &str) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut board| board.set_text(text.to_string()))
        .map_err(|err| err.to_string())
}

/// The text on the clipboard.
#[cfg(not(target_os = "android"))]
pub fn get() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut board| board.get_text())
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "android")]
pub fn set(text: &str) -> Result<(), String> {
    crate::android::set_clipboard(text)
}

#[cfg(target_os = "android")]
pub fn get() -> Result<String, String> {
    crate::android::clipboard()
}
