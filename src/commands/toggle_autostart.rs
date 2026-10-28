//! Toggles the current user's "Start with Windows" entry.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::features::autostart;

pub struct ToggleAutostart;

impl Command for ToggleAutostart {
    fn id(&self) -> CommandId {
        ids::TOGGLE_AUTOSTART
    }

    fn label(&self) -> &'static str {
        "Start with Windows"
    }

    fn checked(&self) -> bool {
        autostart::is_enabled()
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        let enable = !autostart::is_enabled();
        match autostart::set_enabled(enable) {
            Ok(()) => ctx.notify(if enable {
                "bt-volume will now start with Windows."
            } else {
                "bt-volume will no longer start with Windows."
            }),
            Err(err) => ctx.notify_warning(format!(
                "Could not change the startup setting: {err}"
            )),
        }
        CommandOutcome::Done
    }
}
