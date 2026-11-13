//! SketchCraft: draw an outline, pull it up.
//!
//! The engine is a plain library. Every action is a JSON command run through
//! [`commands::Engine`], so the desktop app, scripts, the CLI and AI agents
//! all drive exactly the same thing.

pub mod assets;
pub mod commands;
pub mod components;
pub mod export;
pub mod geom;
pub mod http;
pub mod import;
pub mod live;
pub mod mcp;
pub mod model;
pub mod render;
pub mod scene;
pub mod shapes;
pub mod topology;
pub mod transform;

pub use commands::{Command, Engine};
pub use model::{Id, Model};
