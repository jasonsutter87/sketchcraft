//! Built-in asset library. Each asset is a command script (core/assets/*.jsonl)
//! run in a scratch engine; the result becomes a component, so every door you
//! place shares one definition.

use crate::model::{Definition, Id, Model};
use crate::Engine;
use anyhow::{bail, Result};

pub struct Asset {
    pub key: &'static str,
    pub name: &'static str,
    pub about: &'static str,
    pub script: &'static str,
}

pub const ASSETS: &[Asset] = &[
    Asset { key: "door", name: "Door", about: "Panel door 1.0 × 2.1 m", script: include_str!("../assets/door.jsonl") },
    Asset { key: "window", name: "Window", about: "Framed window 1.2 × 1.5 m", script: include_str!("../assets/window.jsonl") },
    Asset { key: "garage_door", name: "Garage door", about: "Sectional door 2.7 × 2.2 m", script: include_str!("../assets/garage_door.jsonl") },
    Asset { key: "tree", name: "Tree", about: "Street tree, 4.5 m", script: include_str!("../assets/tree.jsonl") },
    Asset { key: "palm", name: "Palm", about: "Canary Island palm, 7.5 m", script: include_str!("../assets/palm.jsonl") },
    Asset { key: "car", name: "Car", about: "Compact car 4.4 m", script: include_str!("../assets/car.jsonl") },
    Asset { key: "streetlight", name: "Streetlight", about: "6 m pole with arm", script: include_str!("../assets/streetlight.jsonl") },
    Asset { key: "bench", name: "Bench", about: "Park bench 1.8 m", script: include_str!("../assets/bench.jsonl") },
    Asset { key: "hydrant", name: "Hydrant", about: "Fire hydrant", script: include_str!("../assets/hydrant.jsonl") },
];

pub fn find(key: &str) -> Option<&'static Asset> {
    let k = key.to_ascii_lowercase().replace(' ', "_");
    ASSETS.iter().find(|a| a.key == k || a.name.eq_ignore_ascii_case(key))
}

/// Build an asset's geometry by running its script.
pub fn build(asset: &Asset) -> Result<Model> {
    let mut e = Engine::new();
    for (n, line) in asset.script.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let r = e.exec_json(line);
        if r["ok"] != true {
            bail!("asset {} line {}: {}", asset.key, n + 1, r["error"]);
        }
    }
    Ok(e.model)
}

/// The component for an asset in `m`, building it on first use.
pub fn definition(m: &mut Model, key: &str) -> Result<Id> {
    let Some(asset) = find(key) else { bail!("no asset {key}; try: {}", ASSETS.iter().map(|a| a.key).collect::<Vec<_>>().join(", ")) };
    if let Some(id) = m.find_definition(asset.name) {
        return Ok(id);
    }
    let model = build(asset)?;
    let id = m.new_id();
    m.definitions.insert(id, Definition { name: asset.name.to_string(), model: std::sync::Arc::new(model) });
    Ok(id)
}
