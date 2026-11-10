//! The yes/no prompt, through rfd: a portal or zenity dialog on GNOME, an alert on macOS.

use rfd::{MessageButtons, MessageDialog, MessageDialogResult, MessageLevel};

pub fn confirm(title: &str, message: &str) -> bool {
    let answer = MessageDialog::new()
        .set_title(title)
        .set_description(message)
        .set_level(MessageLevel::Info)
        .set_buttons(MessageButtons::YesNo)
        .show();

    answer == MessageDialogResult::Yes
}
