//! Ends the process: the tray message loop and the UI loop both stop.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::features::ui;

pub struct Exit;

impl Command for Exit {
    fn id(&self) -> CommandId {
        ids::EXIT
    }

    fn label(&self) -> &'static str {
        "Exit"
    }

    fn run(&self, _ctx: &CommandContext) -> CommandOutcome {
        ui::request_exit();
        CommandOutcome::Exit
    }
}
