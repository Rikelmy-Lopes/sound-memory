//! The command pattern used by every user-visible feature.
//!
//! A [`Command`] is a small, stateless object that knows its own identifier,
//! menu label and how to execute itself against a [`CommandContext`].  The tray
//! menu and the devices window only know command ids, so adding a feature means
//! adding one file under `src/commands` and registering it in
//! [`CommandRegistry`].

use std::collections::BTreeMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::core::state::Shared;
use crate::features::audio::AudioRequest;

/// Stable identifier used by menus, buttons and the registry.
pub type CommandId = &'static str;

/// What the caller should do after a command has run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandOutcome {
    /// Nothing special; keep running.
    Done,
    /// The command asked the process to stop.
    Exit,
}

/// Destination for user notifications (tray balloons).
///
/// The audio engine and the devices window have no notifier, so [`CommandContext`]
/// simply drops notifications when none is installed.
pub trait Notifier {
    fn notify(&self, text: &str, warning: bool);
}

/// Everything a command is allowed to touch.
pub struct CommandContext {
    shared: Arc<Shared>,
    audio: Sender<AudioRequest>,
    notifier: Option<Box<dyn Notifier>>,
}

impl CommandContext {
    pub fn new(shared: Arc<Shared>, audio: Sender<AudioRequest>) -> Self {
        Self {
            shared,
            audio,
            notifier: None,
        }
    }

    /// Installs the tray notifier used by commands such as *Show status*.
    pub fn with_notifier(mut self, notifier: Box<dyn Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn shared(&self) -> &Shared {
        &self.shared
    }

    pub fn audio(&self) -> &Sender<AudioRequest> {
        &self.audio
    }

    pub fn notify(&self, text: impl AsRef<str>) {
        if let Some(notifier) = &self.notifier {
            notifier.notify(text.as_ref(), false);
        }
    }

    pub fn notify_warning(&self, text: impl AsRef<str>) {
        if let Some(notifier) = &self.notifier {
            notifier.notify(text.as_ref(), true);
        }
    }
}

/// One feature, exposed as an executable command.
pub trait Command: Send + Sync {
    fn id(&self) -> CommandId;

    /// Text shown in the tray menu and (optionally) on buttons.
    fn label(&self) -> &'static str;

    /// Whether a check mark should be shown next to the label.
    fn checked(&self) -> bool {
        false
    }

    fn run(&self, ctx: &CommandContext) -> CommandOutcome;
}

/// All commands available to the application, keyed by id.
#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<CommandId, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, command: impl Command + 'static) -> &mut Self {
        self.commands.insert(command.id(), Box::new(command));
        self
    }

    pub fn get(&self, id: CommandId) -> Option<&dyn Command> {
        self.commands.get(id).map(|command| command.as_ref())
    }

    pub fn run(&self, id: CommandId, ctx: &CommandContext) -> Option<CommandOutcome> {
        self.get(id).map(|command| command.run(ctx))
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Noop;

    impl Command for Noop {
        fn id(&self) -> CommandId {
            "noop"
        }

        fn label(&self) -> &'static str {
            "No-op"
        }

        fn run(&self, _ctx: &CommandContext) -> CommandOutcome {
            CommandOutcome::Done
        }
    }

    #[test]
    fn registry_runs_commands_by_id() {
        let mut registry = CommandRegistry::new();
        registry.register(Noop);

        assert_eq!(registry.len(), 1);
        assert_eq!(registry.get("noop").unwrap().label(), "No-op");
        assert!(registry.get("missing").is_none());
    }
}
