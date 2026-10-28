//! Opens the diagnostic log in the default editor.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::core::util;

pub struct OpenLogFile;

impl Command for OpenLogFile {
    fn id(&self) -> CommandId {
        ids::OPEN_LOG_FILE
    }

    fn label(&self) -> &'static str {
        "Open log file"
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        util::open_path(&ctx.shared().log_file);
        CommandOutcome::Done
    }
}
