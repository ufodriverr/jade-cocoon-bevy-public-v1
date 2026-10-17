//! The system's message windows: the questions and the errors that a PC asks and says in
//! a window of its own (`rfd`). A phone has no such windows (`android.rs`): a question
//! there is answered yes and an error goes to the log, which `adb logcat` shows.

/// The title of every window.
const TITLE: &str = "Jade Cocoon";

/// Asks a yes-or-no question and waits for the answer: whether it was yes.
#[cfg(not(target_os = "android"))]
pub fn ask_yes_no(question: &str) -> bool {
    use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};
    let said = MessageDialog::new()
        .set_title(TITLE)
        .set_level(MessageLevel::Warning)
        .set_description(question)
        .set_buttons(MessageButtons::YesNo)
        .show();
    said == MessageDialogResult::Yes
}

#[cfg(target_os = "android")]
pub fn ask_yes_no(question: &str) -> bool {
    bevy::log::info!("{TITLE}: {question} (a phone has no question window: yes)");
    true
}

/// Says an error and waits until it is read.
#[cfg(not(target_os = "android"))]
pub fn tell_error(text: &str) {
    use rfd::{MessageButtons, MessageDialog, MessageLevel};
    MessageDialog::new()
        .set_title(TITLE)
        .set_level(MessageLevel::Error)
        .set_description(text)
        .set_buttons(MessageButtons::Ok)
        .show();
}

#[cfg(target_os = "android")]
pub fn tell_error(text: &str) {
    bevy::log::error!("{TITLE}: {text}");
}
