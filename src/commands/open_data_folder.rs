//! Opens the per-user data folder in Explorer.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::core::util;

pub struct OpenDataFolder;

impl Command for OpenDataFolder {
    fn id(&self) -> CommandId {
        ids::OPEN_DATA_FOLDER
    }

    fn label(&self) -> &'static str {
        "Open data folder"
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        util::open_path(&ctx.shared().data_dir);
        CommandOutcome::Done
    }
}
