//! Riseline: draw an outline, pull it up.
//!
//! The engine is a plain library. Every action is a JSON command run through
//! [`commands::Engine`], so the desktop app, scripts, the CLI and AI agents
//! all drive exactly the same thing.

pub mod commands;
pub mod export;
pub mod geom;
pub mod mcp;
pub mod model;
pub mod render;

pub use commands::{Command, Engine};
pub use model::{Id, Model};
