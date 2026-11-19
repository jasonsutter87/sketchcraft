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
    /// a flat figure that turns to face the camera
    pub face_me: bool,
}

pub const ASSETS: &[Asset] = &[
    Asset { key: "door", name: "Door", about: "Panel door 1.0 × 2.1 m", script: include_str!("../assets/door.jsonl"), face_me: false },
    Asset { key: "window", name: "Window", about: "Framed window 1.2 × 1.5 m", script: include_str!("../assets/window.jsonl"), face_me: false },
    Asset { key: "garage_door", name: "Garage door", about: "Sectional door 2.7 × 2.2 m", script: include_str!("../assets/garage_door.jsonl"), face_me: false },
    Asset { key: "tree", name: "Tree", about: "Street tree, 4.5 m", script: include_str!("../assets/tree.jsonl"), face_me: false },
    Asset { key: "palm", name: "Palm", about: "Canary Island palm, 7.5 m", script: include_str!("../assets/palm.jsonl"), face_me: false },
    Asset { key: "car", name: "Car", about: "Compact car 4.4 m", script: include_str!("../assets/car.jsonl"), face_me: false },
    Asset { key: "streetlight", name: "Streetlight", about: "6 m pole with arm", script: include_str!("../assets/streetlight.jsonl"), face_me: false },
    Asset { key: "bench", name: "Bench", about: "Park bench 1.8 m", script: include_str!("../assets/bench.jsonl"), face_me: false },
    Asset { key: "person", name: "Person", about: "Scale figure 1.75 m, always faces you", script: include_str!("../assets/person.jsonl"), face_me: true },
    Asset { key: "beaver", name: "Beaver", about: "The SketchCraft beaver with a clipboard, 1.5 m, always faces you", script: include_str!("../assets/beaver.jsonl"), face_me: true },
    Asset { key: "hydrant", name: "Hydrant", about: "Fire hydrant", script: include_str!("../assets/hydrant.jsonl"), face_me: false },
    // the texturing test set (tools/texturing_set/texturing_set.py, docs/dev/texturing-test-set.md); the beaver is the sixth
    Asset { key: "dice", name: "Dice", about: "Dice 0.05 × 0.05 × 0.05 m, six separate faces", script: include_str!("../assets/dice.jsonl"), face_me: false },
    Asset { key: "can", name: "Can", about: "Drinks can 0.066 × 0.066 × 0.122 m, 32 sides", script: include_str!("../assets/can.jsonl"), face_me: false },
    Asset { key: "globe", name: "Globe", about: "Globe 0.30 × 0.30 × 0.35 m on a stand", script: include_str!("../assets/globe.jsonl"), face_me: false },
    Asset { key: "crate", name: "Crate", about: "Wooden crate 0.60 × 0.60 × 0.60 m of separate planks", script: include_str!("../assets/crate.jsonl"), face_me: false },
    Asset { key: "soccer_ball", name: "Soccer ball", about: "Soccer ball 0.22 m, 12 pentagons and 20 hexagons", script: include_str!("../assets/soccer_ball.jsonl"), face_me: false },
    // fixtures and furniture: fixed assets, back edge on y=0, front toward -Y (see docs/dev/fixtures.md)
    Asset { key: "toilet", name: "Toilet", about: "Toilet 0.38 × 0.70 × 0.78 m, back against a wall", script: include_str!("../assets/toilet.jsonl"), face_me: false },
    Asset { key: "vanity_60", name: "Vanity 0.6 m", about: "Vanity 0.6 m 0.60 × 0.50 × 0.95 m, back against a wall", script: include_str!("../assets/vanity_60.jsonl"), face_me: false },
    Asset { key: "vanity_120", name: "Vanity 1.2 m", about: "Vanity 1.2 m 1.20 × 0.50 × 0.95 m, back against a wall", script: include_str!("../assets/vanity_120.jsonl"), face_me: false },
    Asset { key: "bathtub", name: "Bathtub", about: "Bathtub 1.50 × 0.70 × 0.60 m, back against a wall", script: include_str!("../assets/bathtub.jsonl"), face_me: false },
    Asset { key: "shower", name: "Shower stall", about: "Shower stall 0.90 × 0.90 × 2.00 m, back against a wall", script: include_str!("../assets/shower.jsonl"), face_me: false },
    Asset { key: "towel_bar", name: "Towel bar", about: "Towel bar 0.60 × 0.08 × 0.06 m, back against a wall", script: include_str!("../assets/towel_bar.jsonl"), face_me: false },
    Asset { key: "base_cabinet", name: "Base cabinet", about: "Base cabinet 0.60 × 0.60 × 0.90 m, back against a wall", script: include_str!("../assets/base_cabinet.jsonl"), face_me: false },
    Asset { key: "sink_base", name: "Sink base", about: "Sink base 0.80 × 0.60 × 1.10 m, back against a wall", script: include_str!("../assets/sink_base.jsonl"), face_me: false },
    Asset { key: "wall_cabinet", name: "Wall cabinet", about: "Wall cabinet 0.60 × 0.32 × 0.72 m, back against a wall", script: include_str!("../assets/wall_cabinet.jsonl"), face_me: false },
    Asset { key: "pantry", name: "Pantry cabinet", about: "Pantry cabinet 0.60 × 0.60 × 2.10 m, back against a wall", script: include_str!("../assets/pantry.jsonl"), face_me: false },
    Asset { key: "fridge", name: "Fridge", about: "Fridge 0.76 × 0.70 × 1.78 m, back against a wall", script: include_str!("../assets/fridge.jsonl"), face_me: false },
    Asset { key: "range", name: "Range", about: "Range 0.76 × 0.65 × 0.92 m, back against a wall", script: include_str!("../assets/range.jsonl"), face_me: false },
    Asset { key: "dishwasher", name: "Dishwasher", about: "Dishwasher 0.60 × 0.60 × 0.84 m, back against a wall", script: include_str!("../assets/dishwasher.jsonl"), face_me: false },
    Asset { key: "island", name: "Kitchen island", about: "Kitchen island 1.80 × 0.90 × 0.92 m, back against a wall", script: include_str!("../assets/island.jsonl"), face_me: false },
    Asset { key: "queen_bed", name: "Queen bed", about: "Queen bed 1.62 × 2.13 × 1.10 m, back against a wall", script: include_str!("../assets/queen_bed.jsonl"), face_me: false },
    Asset { key: "king_bed", name: "King bed", about: "King bed 2.03 × 2.13 × 1.10 m, back against a wall", script: include_str!("../assets/king_bed.jsonl"), face_me: false },
    Asset { key: "twin_bed", name: "Twin bed", about: "Twin bed 1.09 × 2.00 × 0.95 m, back against a wall", script: include_str!("../assets/twin_bed.jsonl"), face_me: false },
    Asset { key: "nightstand", name: "Nightstand", about: "Nightstand 0.50 × 0.40 × 0.55 m, back against a wall", script: include_str!("../assets/nightstand.jsonl"), face_me: false },
    Asset { key: "dresser", name: "Dresser", about: "Dresser 1.20 × 0.50 × 0.85 m, back against a wall", script: include_str!("../assets/dresser.jsonl"), face_me: false },
    Asset { key: "sofa", name: "Sofa, 3-seat", about: "Sofa, 3-seat 2.10 × 0.92 × 0.85 m, back against a wall", script: include_str!("../assets/sofa.jsonl"), face_me: false },
    Asset { key: "armchair", name: "Armchair", about: "Armchair 0.90 × 0.88 × 0.85 m, back against a wall", script: include_str!("../assets/armchair.jsonl"), face_me: false },
    Asset { key: "coffee_table", name: "Coffee table", about: "Coffee table 1.10 × 0.60 × 0.45 m, back against a wall", script: include_str!("../assets/coffee_table.jsonl"), face_me: false },
    Asset { key: "tv_stand", name: "TV stand", about: "TV stand 1.50 × 0.40 × 0.50 m, back against a wall", script: include_str!("../assets/tv_stand.jsonl"), face_me: false },
    Asset { key: "dining_set", name: "Dining table with 4 chairs", about: "Dining table with 4 chairs 1.50 × 1.62 × 0.90 m, back against a wall", script: include_str!("../assets/dining_set.jsonl"), face_me: false },
    Asset { key: "washer", name: "Washer", about: "Washer 0.69 × 0.74 × 0.98 m, back against a wall", script: include_str!("../assets/washer.jsonl"), face_me: false },
    Asset { key: "dryer", name: "Dryer", about: "Dryer 0.69 × 0.74 × 0.98 m, back against a wall", script: include_str!("../assets/dryer.jsonl"), face_me: false },
    // scale figures for paths and streets (tools/scale_figures/scale_figures.py)
    Asset { key: "walker", name: "Walker", about: "Scale figure 1.78 m walking, always faces you", script: include_str!("../assets/walker.jsonl"), face_me: true },
    Asset { key: "shopper", name: "Shopper", about: "Scale figure 1.66 m with a tote bag, always faces you", script: include_str!("../assets/shopper.jsonl"), face_me: true },
    Asset { key: "jogger", name: "Jogger", about: "Scale figure 1.70 m running, always faces you", script: include_str!("../assets/jogger.jsonl"), face_me: true },
    Asset { key: "child", name: "Child", about: "Scale figure 1.15 m, always faces you", script: include_str!("../assets/child.jsonl"), face_me: true },
    Asset { key: "dog_walker", name: "Dog walker", about: "Scale figure walking a dog, 1.75 m, always faces you", script: include_str!("../assets/dog_walker.jsonl"), face_me: true },
    Asset { key: "stroller", name: "Stroller", about: "Scale figure pushing a stroller, 1.70 m, always faces you", script: include_str!("../assets/stroller.jsonl"), face_me: true },
    Asset { key: "wheelchair", name: "Wheelchair user", about: "Scale figure in a wheelchair, 1.30 m, always faces you", script: include_str!("../assets/wheelchair.jsonl"), face_me: true },
    Asset { key: "cyclist", name: "Cyclist", about: "Scale figure on a bike, 1.75 m long, always faces you", script: include_str!("../assets/cyclist.jsonl"), face_me: true },
    Asset { key: "car_flat", name: "Car (flat)", about: "Flat hatchback 4.3 m in profile, for elevations; rotate it along the road", script: include_str!("../assets/car_flat.jsonl"), face_me: false },
    Asset { key: "suv_flat", name: "SUV (flat)", about: "Flat SUV 4.7 m in profile, for elevations; rotate it along the road", script: include_str!("../assets/suv_flat.jsonl"), face_me: false },
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

/// A built-in asset's definition carries this reserved `source`. A person, plugin, agent, file or library
/// item can name a component "Door" (or anything else) but doesn't get the built-in's key, and the
/// geometry is checked against a fingerprint of what the asset's own script builds, so a lookalike
/// planted with a forged `source` is never reused either (SPECTER-001).
pub fn source_of(asset: &Asset) -> String {
    format!("builtin:{}", asset.key)
}

fn fingerprint(model: &Model) -> String {
    crate::safety::sha256_hex(&serde_json::to_vec(model).unwrap_or_default())
}

fn expected_fingerprint(asset: &'static Asset, model: Option<&Model>) -> Result<String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<std::collections::HashMap<&'static str, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(f) = cache.lock().unwrap().get(asset.key) {
        return Ok(f.clone());
    }
    let f = match model {
        Some(m) => fingerprint(m),
        None => fingerprint(&build(asset)?),
    };
    cache.lock().unwrap().insert(asset.key, f.clone());
    Ok(f)
}

/// The component for an asset in `m`, building it on first use. Found by the reserved key, never by
/// the component's name.
pub fn definition(m: &mut Model, key: &str) -> Result<Id> {
    let Some(asset) = find(key) else { bail!("no asset {key}; try: {}", ASSETS.iter().map(|a| a.key).collect::<Vec<_>>().join(", ")) };
    let src = source_of(asset);
    // doors and windows cut their opening in the wall they're placed on
    let cuts = matches!(asset.key, "door" | "window" | "garage_door");
    let candidates: Vec<Id> = m.definitions.iter().filter(|(_, d)| d.source.as_deref() == Some(src.as_str()) && d.cuts == cuts && d.face_me == asset.face_me).map(|(&k, _)| k).collect();
    let model = if candidates.is_empty() { Some(build(asset)?) } else { None };
    let want = expected_fingerprint(asset, model.as_ref())?;
    if let Some(id) = candidates.into_iter().find(|id| fingerprint(&m.definitions[id].model) == want) {
        return Ok(id);
    }
    let model = match model {
        Some(x) => x,
        None => build(asset)?,
    };
    let id = m.new_id();
    m.definitions.insert(id, Definition { name: asset.name.to_string(), model: std::sync::Arc::new(model), attrs: Default::default(), source: Some(src), cuts, face_me: asset.face_me });
    Ok(id)
}
