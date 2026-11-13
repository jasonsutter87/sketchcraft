//! SketchCraft: draw an outline, pull it up.
//!
//! The engine is a plain library. Every action is a JSON command run through
//! [`commands::Engine`], so the desktop app, scripts, the CLI and AI agents
//! all drive exactly the same thing.

pub mod animate;
pub mod assets;
pub mod builders;
pub mod commands;
pub mod components;
pub mod csg;
pub mod export;
pub mod export2;
pub mod fbx;
pub mod geom;
pub mod http;
pub mod import;
pub mod intersect;
pub mod live;
pub mod mcp;
pub mod mesh;
pub mod matchphoto;
pub mod model;
pub mod render;
pub mod sandbox;
pub mod scene;
pub mod section;
pub mod shapes;
pub mod skp;
pub mod skp_vff;
pub mod topology;
pub mod transform;
pub mod units;

pub use commands::{Command, Engine};
pub use model::{Id, Model};
