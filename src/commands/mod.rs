//! Command system — Vim-like command palette.
//!
//! Commands are invoked via `:` prefix. Each command implements
//! the `Command` trait and is registered with the `CommandRegistry`.

use std::collections::HashMap;

use crate::core::traits::Command;

/// Registry of all available commands, keyed by name.
pub struct CommandRegistry {
    commands: HashMap<String, Box<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// Register a command directly by name.
    pub fn register(&mut self, name: String, command: Box<dyn Command>) -> anyhow::Result<()> {
        let key = name.to_lowercase();
        if self.commands.contains_key(&key) {
            anyhow::bail!("Command '{}' is already registered", name);
        }
        self.commands.insert(key, command);
        Ok(())
    }

    /// Look up a command by name.
    pub fn find(&self, name: &str) -> Option<&dyn Command> {
        self.commands.get(&name.to_lowercase()).map(|c| c.as_ref())
    }

    /// Execute a command string (e.g., ":play", ":volume 80").
    pub fn execute(&self, input: &str) -> anyhow::Result<()> {
        let input = input.strip_prefix(':').unwrap_or(input);
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            anyhow::bail!("Empty command");
        }

        let cmd_name = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        match self.find(cmd_name) {
            Some(cmd) => cmd.execute(&args),
            None => anyhow::bail!("Unknown command: {}", cmd_name),
        }
    }

    /// Returns all command names for completion.
    pub fn command_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.commands.keys().map(|s| s.as_str()).collect();
        names.sort();
        names.dedup();
        names
    }

    /// Returns completion candidates for a command prefix.
    pub fn completions(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.strip_prefix(':').unwrap_or(prefix).to_lowercase();
        self.commands
            .keys()
            .filter(|name| name.starts_with(&prefix))
            .map(|s| format!(":{}", s))
            .collect()
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::traits::Command;

    struct TestCommand;

    impl Command for TestCommand {
        fn name(&self) -> &str { "test" }
        fn description(&self) -> &str { "A test command" }
        fn usage(&self) -> &str { ":test <arg>" }
        fn execute(&self, args: &[String]) -> anyhow::Result<()> {
            let _ = args;
            Ok(())
        }
    }

    #[test]
    fn test_registry_find() {
        let mut registry = CommandRegistry::new();
        registry.register("test".to_string(), Box::new(TestCommand)).unwrap();
        assert!(registry.find("test").is_some());
        assert!(registry.find("unknown").is_none());
    }

    #[test]
    fn test_completions() {
        let mut registry = CommandRegistry::new();
        registry.register("test".to_string(), Box::new(TestCommand)).unwrap();
        let completions = registry.completions("te");
        assert!(!completions.is_empty());
    }

    #[test]
    fn test_duplicate_register_fails() {
        let mut registry = CommandRegistry::new();
        registry.register("test".to_string(), Box::new(TestCommand)).unwrap();
        assert!(registry.register("test".to_string(), Box::new(TestCommand)).is_err());
    }

    #[test]
    fn test_execute_empty() {
        let registry = CommandRegistry::new();
        assert!(registry.execute("").is_err());
    }
}
