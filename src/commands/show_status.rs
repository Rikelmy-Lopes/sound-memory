//! Shows a tray balloon with the current volume of up to four devices.

use crate::commands::ids;
use crate::core::command::{Command, CommandContext, CommandId, CommandOutcome};
use crate::core::util;

pub struct ShowStatus;

impl Command for ShowStatus {
    fn id(&self) -> CommandId {
        ids::SHOW_STATUS
    }

    fn label(&self) -> &'static str {
        "Show status"
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome {
        let devices = ctx
            .shared()
            .devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut text = String::new();
        if devices.is_empty() {
            text.push_str(
                "No Bluetooth audio device is connected.\nSaved levels will be restored when one connects.",
            );
        } else {
            for device in devices.iter().take(4) {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!(
                    "{}: {}%",
                    util::truncate(&device.name, 34),
                    device.level_pct
                ));
            }
            if devices.len() > 4 {
                text.push_str(&format!("\n…and {} more", devices.len() - 4));
            }
        }
        drop(devices);

        ctx.notify(text);
        CommandOutcome::Done
    }
}
