//! SketchCraft: draw an outline, pull it up.
//!
//! The engine is a plain library. Every action is a JSON command run through
//! [`commands::Engine`], so the desktop app, scripts, the CLI and AI agents
//! all drive exactly the same thing.

pub mod anchors;
pub mod animate;
pub mod backups;
pub mod assets;
pub mod builders;
pub mod callouts;
pub mod cloud;
pub mod commands;
pub mod components;
pub mod csg;
pub mod drafting;
pub mod draft;
pub mod dynamic;
pub mod dwg;
pub mod dxf;
pub mod edges;
pub mod entities;
pub mod environment;
pub mod export;
pub mod export2;
pub mod fbx;
pub mod format;
pub mod geo;
pub mod helper;
pub mod geom;
pub mod history;
pub mod http;
pub mod ifc;
pub mod ifcin;
pub mod import;
pub mod intersect;
pub mod layout;
pub mod library;
pub mod live;
pub mod mcp;
pub mod mesh;
pub mod matchphoto;
pub mod model;
pub mod nesting;
pub mod openings;
pub mod photomodel;
pub mod photoproject;
pub mod pt;
pub mod render;
pub mod rectpoly;
pub mod room;
pub mod sandbox;
pub mod safety;
pub mod scene;
pub mod plugins;
pub mod policy;
pub mod section;
pub mod shapes;
pub mod skp;
pub mod subd;
pub mod texmap;
pub mod timeline;
pub mod skp_vff;
pub mod topology;
pub mod transform;
pub mod units;
pub mod wallpull;

pub use commands::{Command, Engine};
pub use model::{Id, Model};
