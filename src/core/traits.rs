//! Core traits — foundational abstractions for the entire application.
//!
//! Every component, widget, plugin, and command implements
//! one or more traits defined here.

use crate::core::id::ComponentId;
use crate::events::Event;

/// A named, identifiable component that can be started and shut down.
pub trait Component: Send + Sync {
    fn id(&self) -> ComponentId;

    fn name(&self) -> &str;

    /// Called once when the component is registered with the runtime.
    fn on_start(&mut self) {}

    /// Called when the application is shutting down.
    fn on_shutdown(&mut self) {}

    /// Handle an event; return true if the event was consumed.
    fn handle_event(&mut self, event: &Event) -> bool {
        let _ = event;
        false
    }
}

/// A component that runs an async event loop in the background.
#[async_trait::async_trait]
pub trait AsyncComponent: Component {
    async fn run(&mut self);
}

/// Marks a component that can be paused and resumed (e.g., audio playback).
pub trait Pausable {
    fn pause(&mut self);
    fn resume(&mut self);
    fn is_paused(&self) -> bool;
}

/// A command that can be invoked via the command palette (`:`).
pub trait Command: Send + Sync {
    fn name(&self) -> &str;

    fn aliases(&self) -> &[&str] {
        &[]
    }

    fn description(&self) -> &str;

    fn usage(&self) -> &str;

    fn execute(&self, args: &[String]) -> anyhow::Result<()>;

    fn completions(&self, prefix: &str) -> Vec<String> {
        let _ = prefix;
        vec![]
    }
}

/// A plugin that extends Tanu functionality.
///
/// Lifecycle: `on_init` → [`on_event`, `on_tick`]… → `on_shutdown`
///
/// All hooks receive a `&PluginContext` for controlled access
/// to app services (event bus, database, config, storage).
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn author(&self) -> &str;
    fn description(&self) -> &str;

    /// Called once after registration. Use `ctx` to query config or emit events.
    fn on_init(&mut self, _ctx: &crate::plugins::PluginContext) {}

    /// Called for every global event. Return `true` if the event was consumed.
    fn on_event(&mut self, _ctx: &crate::plugins::PluginContext, _event: &Event) -> bool {
        false
    }

    /// Called periodically (e.g., every 1s) from the main loop.
    /// Use for background tasks like scrobbling, presence updates, etc.
    fn on_tick(&mut self, _ctx: &crate::plugins::PluginContext) {}

    /// Called when the plugin is unregistered or the application shuts down.
    fn on_shutdown(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::id::ComponentId;

    struct TestComponent {
        id: ComponentId,
    }

    impl Component for TestComponent {
        fn id(&self) -> ComponentId {
            self.id
        }
        fn name(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn test_component_id() {
        let comp = TestComponent {
            id: ComponentId::new(),
        };
        assert_eq!(comp.name(), "test");
    }

    #[test]
    fn test_component_handle_event_returns_false_by_default() {
        let mut comp = TestComponent {
            id: ComponentId::new(),
        };
        let consumed = comp.handle_event(&crate::events::Event::Quit);
        assert!(!consumed);
    }

    struct EchoCommand;

    impl Command for EchoCommand {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes back the arguments"
        }
        fn usage(&self) -> &str {
            ":echo <text>"
        }
        fn execute(&self, args: &[String]) -> anyhow::Result<()> {
            println!("{}", args.join(" "));
            Ok(())
        }
    }

    #[test]
    fn test_command_aliases_default_empty() {
        let cmd = EchoCommand;
        assert!(cmd.aliases().is_empty());
    }

    #[test]
    fn test_command_completions_default_empty() {
        let cmd = EchoCommand;
        assert!(cmd.completions("").is_empty());
    }
}
