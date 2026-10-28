//! Asks the audio engine to re-apply every saved volume.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::features::audio::AudioRequest;

pub struct RestoreAll;

impl Command for RestoreAll {
    fn id(&self) -> CommandId {
        ids::RESTORE_ALL
    }

    fn label(&self) -> &'static str {
        "Restore saved volumes"
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        if ctx.audio().send(AudioRequest::RestoreAll).is_ok() {
            ctx.notify("Restoring saved volume levels…");
        }
        CommandOutcome::Done
    }
}
