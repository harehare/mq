/// Process-execution access granted to [`SandboxedIo`](super::SandboxedIo); same shape as
/// [`PathAccess`](super::PathAccess)/[`EnvAccess`](super::EnvAccess) but keyed by command name
/// (e.g. `mq-run`'s `--allow-run`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RunAccess {
    /// No process execution at all (fail-safe default).
    #[default]
    Denied,
    /// Unrestricted access, matching a bare `--allow-run`.
    Allowed,
    /// Execution restricted to the given command names.
    AllowedCommands(Vec<String>),
}

impl From<bool> for RunAccess {
    fn from(allow: bool) -> Self {
        if allow { RunAccess::Allowed } else { RunAccess::Denied }
    }
}

impl From<Vec<String>> for RunAccess {
    fn from(commands: Vec<String>) -> Self {
        if commands.is_empty() {
            RunAccess::Allowed
        } else {
            RunAccess::AllowedCommands(commands)
        }
    }
}

impl From<Option<Vec<String>>> for RunAccess {
    fn from(commands: Option<Vec<String>>) -> Self {
        match commands {
            None => RunAccess::Denied,
            Some(commands) => commands.into(),
        }
    }
}

impl RunAccess {
    pub(super) fn permits(&self, command: &str) -> bool {
        match self {
            RunAccess::Denied => false,
            RunAccess::Allowed => true,
            RunAccess::AllowedCommands(allowed) => allowed.iter().any(|allowed_command| allowed_command == command),
        }
    }

    pub(super) fn is_denied(&self) -> bool {
        matches!(self, RunAccess::Denied)
    }
}
