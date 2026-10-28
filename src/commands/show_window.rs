//! Opens (or focuses) the optional egui devices window.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::features::ui;

pub struct ShowWindow;

impl Command for ShowWindow {
    fn id(&self) -> CommandId {
        ids::SHOW_WINDOW
    }

    fn label(&self) -> &'static str {
        "Devices window…"
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        if ui::is_available() {
            ui::request_show();
        } else {
            ctx.notify_warning("The devices window is not available in this session.");
        }
        CommandOutcome::Done
    }
}
