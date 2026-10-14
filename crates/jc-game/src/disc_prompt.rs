//! The question for the disc image (user's request, 2026-10-13): a play session that has
//! no stored path, or whose stored image is gone or is not the game's disc, asks for the
//! image with the system's file window and stores the answer in `jc.local.toml`, so the
//! next start finds it. A run that ends on its own never asks.

use std::path::Path;

use jc_disc::{ConfigError, LOCAL_CONFIG_NAME, LocalConfig};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

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
            .ok_or("no disc image was chosen")?;
        match LocalConfig::disc_problem(&image) {
            Some(wrong) => why = wrong,
            None => return store(&start, &image),
        }
    }
}

/// Stores a good image and looks whether a start reads it now: a `jc.local.toml` that
/// is broken in another line still stops it.
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
        assert!(text.contains(LOCAL_CONFIG_NAME));
    }
}
