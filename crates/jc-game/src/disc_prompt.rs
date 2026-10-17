//! The question for the disc image (user's request, 2026-10-13): a play session that has
//! no stored path, or whose stored image is gone or is not the game's disc, asks for the
//! image with the system's file window and stores the answer in `jc.local.toml`, so the
//! next start finds it. A run that ends on its own never asks.
//!
//! A phone asks before the game starts (`android/`, the first screen of the APK, which
//! copies the chosen image into the app's own folder): the game there only checks the
//! image, and a wrong one goes back to that screen with the reason (`android.rs`).

#[cfg(not(target_os = "android"))]
use std::path::Path;

#[cfg(not(target_os = "android"))]
use jc_disc::LOCAL_CONFIG_NAME;
use jc_disc::{ConfigError, LocalConfig};
#[cfg(not(target_os = "android"))]
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

#[cfg(not(target_os = "android"))]
const TITLE: &str = "Jade Cocoon";

/// What keeps the stored disc image from being used; `None` when a start can read it.
fn problem() -> Option<String> {
    match LocalConfig::load() {
        Ok(config) => LocalConfig::disc_problem(&config.disc_image),
        Err(ConfigError::NotFound(_)) => Some("No disc image has been chosen yet.".to_string()),
        Err(err) => Some(err.to_string()),
    }
}

/// What the first window says: why it is there and what the next one wants.
#[cfg(any(not(target_os = "android"), test))]
fn question(why: &str) -> String {
    format!(
        "{why}\n\nThe game plays from your own disc image of Jade Cocoon: Story of the \
         Tamamayu (Europe, SLES-02201), a raw .bin file. Choose it in the next window; the \
         path is kept in {LOCAL_CONFIG_NAME} beside the game and checked at every start.\n\n\
         You must lawfully possess an original game copy and have the right to use it."
    )
}

/// Makes sure a start can read the disc: nothing to do with a stored image that is still
/// there; otherwise the question, again for a file that is not the disc, until an image
/// is stored or the person gives up, which is the error.
#[cfg(not(target_os = "android"))]
pub fn ensure() -> Result<(), String> {
    let Some(mut why) = problem() else {
        return Ok(());
    };
    let start = std::env::current_dir().map_err(|err| err.to_string())?;
    loop {
        let goes_on = MessageDialog::new()
            .set_title(TITLE)
            .set_level(MessageLevel::Info)
            .set_description(question(&why))
            .set_buttons(MessageButtons::OkCancel)
            .show();
        let image = (goes_on == MessageDialogResult::Ok)
            .then(|| {
                FileDialog::new()
                    .set_title("Choose your disc image of Jade Cocoon")
                    .add_filter("Disc image", &["bin", "img", "iso"])
                    .add_filter("All files", &["*"])
                    .pick_file()
            })
            .flatten()
            .ok_or(GIVEN_UP)?;
        match LocalConfig::disc_problem(&image) {
            Some(wrong) => why = wrong,
            None => return store(&start, &image),
        }
    }
}

/// What was given up: why the start goes no further. The person who said no needs no
/// window for it; any other reason is said in one, since a release exe has no console
/// to print it in.
#[cfg(not(target_os = "android"))]
pub fn cannot_start(err: &str) {
    if err == GIVEN_UP {
        return;
    }
    MessageDialog::new()
        .set_title(TITLE)
        .set_level(MessageLevel::Error)
        .set_description(format!("Jade Cocoon cannot start:\n\n{err}"))
        .set_buttons(MessageButtons::Ok)
        .show();
}

/// A phone's disc image was chosen and copied before the game started: it is only
/// checked here.
#[cfg(target_os = "android")]
pub fn ensure() -> Result<(), String> {
    problem().map_or(Ok(()), Err)
}

/// A phone goes back to the screen that asks for the image, with the reason.
#[cfg(target_os = "android")]
pub fn cannot_start(err: &str) {
    crate::android::disc_refused(err);
}

/// The error of a person who chose no disc image.
#[cfg(not(target_os = "android"))]
const GIVEN_UP: &str = "no disc image was chosen";

/// Stores a good image and looks whether a start reads it now: a `jc.local.toml` that
/// is broken in another line still stops it.
#[cfg(not(target_os = "android"))]
fn store(start: &Path, image: &Path) -> Result<(), String> {
    let file = LocalConfig::store_disc_image(start, image)
        .map_err(|err| format!("cannot write {LOCAL_CONFIG_NAME}: {err}"))?;
    println!(
        "the disc image {} is stored in {}",
        image.display(),
        file.display()
    );
    problem().map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_says_why_and_where_the_answer_is_kept() {
        let text = question("There is no file at X.");
        assert!(text.starts_with("There is no file at X.\n\n"));
        assert!(text.contains(jc_disc::LOCAL_CONFIG_NAME));
    }
}
