//! The file format guard: what a model file says about itself, and what this build can safely do with it.
//!
//! A `.sketchcraft` file is the JSON of a `Model`, plus a top-level `format` number. Unknown fields are ignored when a
//! file is read, so a build that opens a file from a newer SketchCraft and saves it would silently throw away whatever
//! it doesn't know. The guard looks at the file's top level BEFORE the model is read; a file this build can't fully
//! understand opens read-only (see `Engine::read_only`): nothing is ever written back over it.
//!
//! - `format`: 1, or 2 when the model holds drawing sheets (`format_of`). Files from before the guard have none, which
//!   means 1. A file with a higher number than `FORMAT` is from a newer SketchCraft.
//! - `needs`: an optional list of features a reader must understand. Anything not in `KNOWN_NEEDS` is unknown.
//! - top-level keys: any key that isn't in `KNOWN_KEYS` is data this build would lose.

use anyhow::Result;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// The highest format number this build reads for editing. A model with sheets is written as format 2 (see
/// `format_of`); one without stays format 1, so an older SketchCraft (0.3.7 and later read-only, earlier ones as is)
/// can still open it wherever that is possible.
pub const FORMAT: u64 = 2;

/// Features (the `needs` list) this build understands. "sheets": drawing sheets in the model (0.3.8). "face_uv": faces with their own texture coordinates (projection mapping, 0.3.9). "face_vcol": faces with vertex colours. "face_tex": faces with a painted-through picture placement (`tex`).
pub const KNOWN_NEEDS: &[&str] = &["sheets", "face_uv", "face_vcol", "face_tex", "edge_seam", "face_uv2"];

/// Every top-level key this build knows: the serialized fields of `Model` (kept in step by the test
/// `known_keys_match_the_model_struct`, which reads model.rs) and the two the guard adds.
pub const KNOWN_KEYS: &[&str] = &[
    "format", "needs",
    "next_id", "vertices", "edges", "faces", "groups", "materials", "active_group", "tags", "environment", "tag_colors", "definitions",
    "instances", "guides", "photo_points", "cages", "photo_edges", "photo_guides", "views", "timeline", "annotations", "units", "classes",
    "location", "clouds", "sections", "active_section", "axes",
    "drafting", "sheets", "sheet_items", "title_blocks",
];

/// What the person is told, as it appears in the app.
pub const NOTICE: &str = "Saved by a newer SketchCraft \u{2014} update to edit. You can look, but changes won't be saved here.";

/// Why a file opened read-only (one entry per reason), e.g. "its format is 2 (this version reads up to 1)".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnly {
    pub reasons: Vec<String>,
}

impl ReadOnly {
    /// The reply to a command that would change the model, or save it.
    pub fn refusal(&self) -> String {
        format!("This model was saved by a newer SketchCraft ({}), so it is open read-only: changes aren't made and nothing is written to its file. Update SketchCraft to edit it; \"new\" or \"open\" another model to carry on.", self.reasons.join("; "))
    }
}

/// The top level of a file, read without building the whole tree: a model can be hundreds of megabytes.
#[derive(Default)]
struct Skim {
    keys: Vec<String>,
    format: Option<Value>,
    needs: Option<Value>,
}

impl<'de> Deserialize<'de> for Skim {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Skim;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> std::result::Result<Skim, A::Error> {
                let mut s = Skim::default();
                while let Some(k) = m.next_key::<String>()? {
                    match k.as_str() {
                        "format" => s.format = Some(m.next_value()?),
                        "needs" => s.needs = Some(m.next_value()?),
                        _ => {
                            m.next_value::<IgnoredAny>()?;
                        }
                    }
                    s.keys.push(k);
                }
                Ok(s)
            }
        }
        d.deserialize_map(V)
    }
}

/// Look at a file's top level. `None`: this build understands all of it (or it isn't JSON at all, which reading the
/// model reports in its own words). `Some`: it opens read-only, and why.
pub fn check(bytes: &[u8]) -> Option<ReadOnly> {
    let skim: Skim = serde_json::from_slice(bytes).ok()?;
    Some(ReadOnly { reasons: reasons(&skim) }).filter(|r| !r.reasons.is_empty())
}

fn reasons(s: &Skim) -> Vec<String> {
    let mut why = vec![];
    match &s.format {
        None => {}
        Some(Value::Number(n)) if n.as_u64().is_some_and(|f| f <= FORMAT) => {}
        Some(Value::Number(n)) if n.as_u64().is_some() => why.push(format!("its format is {n}, and this version reads up to {FORMAT}")),
        Some(_) => why.push("its format number isn't one this version knows".into()),
    }
    match &s.needs {
        None => {}
        Some(Value::Array(a)) => {
            let unknown: Vec<String> = a.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).filter(|n| !KNOWN_NEEDS.contains(&n.as_str())).collect();
            if !unknown.is_empty() {
                why.push(format!("it needs {}, which this version doesn't have", unknown.join(", ")));
            }
        }
        Some(_) => why.push("its list of needed features isn't one this version can read".into()),
    }
    let mut extra: Vec<&str> = s.keys.iter().map(String::as_str).filter(|k| !KNOWN_KEYS.contains(k)).collect();
    extra.sort_unstable();
    extra.dedup();
    if !extra.is_empty() {
        why.push(format!("it holds {} this version doesn't know", extra.iter().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(", ")));
    }
    why
}

/// Does the model hold anything only a build that knows about sheets can keep? Then the file says so.
fn has_sheets(m: &crate::model::Model) -> bool {
    !m.sheets.is_empty() || !m.sheet_items.is_empty() || !m.title_blocks.is_empty() || !m.drafting.is_empty() || m.has_edge_styles()
        // a model dimension attached to geometry (#59): an older build would read it as fixed points and drop the anchors
        || m.has_attached_dims()
}

/// Does any face (here or in a component) carry its own texture coordinates? An older build would drop them on save.
fn has_face_uv(m: &crate::model::Model) -> bool {
    m.faces.values().any(|f| f.uv.is_some()) || m.definitions.values().any(|d| has_face_uv(&d.model))
}

/// Does any face (here or in a component) carry vertex colours? An older build would drop them on save.
fn has_face_vcol(m: &crate::model::Model) -> bool {
    m.faces.values().any(|f| f.vcol.is_some()) || m.definitions.values().any(|d| has_face_vcol(&d.model))
}

/// Does any face (here or in a component) carry a painted-through picture placement? An older build would drop it on save.
fn has_face_tex(m: &crate::model::Model) -> bool {
    m.faces.values().any(|f| f.tex.is_some()) || m.definitions.values().any(|d| has_face_tex(&d.model))
}

/// Does any edge carry a UV seam mark?
fn has_edge_seam(m: &crate::model::Model) -> bool {
    m.edges.values().any(|e| e.seam) || m.definitions.values().any(|d| has_edge_seam(&d.model))
}

/// Does any face carry a lightmap UV set?
fn has_face_uv2(m: &crate::model::Model) -> bool {
    m.faces.values().any(|f| f.uv2.is_some()) || m.definitions.values().any(|d| has_face_uv2(&d.model))
}

/// The features a file of this model needs a reader to understand.
fn needs_of(m: &crate::model::Model) -> Option<Vec<&'static str>> {
    let mut v = Vec::new();
    if has_sheets(m) {
        v.push("sheets");
    }
    if has_face_uv(m) {
        v.push("face_uv");
    }
    if has_face_vcol(m) {
        v.push("face_vcol");
    }
    if has_face_tex(m) {
        v.push("face_tex");
    }
    if has_edge_seam(m) {
        v.push("edge_seam");
    }
    if has_face_uv2(m) {
        v.push("face_uv2");
    }
    Some(v).filter(|v| !v.is_empty())
}

/// The format number a file of this model is written with: 2 when it holds sheets, per-face texture coordinates or painted-through placements,
/// 1 otherwise.
pub fn format_of(m: &crate::model::Model) -> u64 {
    if needs_of(m).is_some() { 2 } else { 1 }
}

/// A model as a file: the model's own fields with `format` first (and `needs`, when it holds sheets).
#[derive(Serialize)]
struct Stamped<'a> {
    format: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    needs: Option<Vec<&'static str>>,
    #[serde(flatten)]
    model: &'a crate::model::Model,
}

pub fn to_vec(m: &crate::model::Model) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&Stamped { format: format_of(m), needs: needs_of(m), model: m })?)
}

pub fn to_string(m: &crate::model::Model) -> Result<String> {
    Ok(serde_json::to_string(&Stamped { format: format_of(m), needs: needs_of(m), model: m })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The serialized field names of `Model`, read from its source: a field added there without a line in `KNOWN_KEYS`
    /// would make every file this build writes look like a newer one to itself, and fail here first.
    fn model_fields() -> Vec<String> {
        let src = include_str!("model.rs");
        let start = src.find("pub struct Model {").expect("Model struct");
        let body = &src[start..];
        let body = &body[..body.find("\n}\n").expect("end of Model")];
        let mut out = vec![];
        let mut skipped = false;
        for line in body.lines().skip(1) {
            let t = line.trim();
            if t.starts_with("#[serde(skip)]") {
                skipped = true;
            } else if let Some(rest) = t.strip_prefix("pub ") {
                if !skipped {
                    out.push(rest.split(':').next().unwrap().trim().to_string());
                }
                skipped = false;
            }
        }
        out
    }

    #[test]
    fn known_keys_match_the_model_struct() {
        let mut fields = model_fields();
        assert!(fields.len() > 20, "couldn't read the Model fields: {fields:?}");
        fields.extend(["format".to_string(), "needs".to_string()]);
        fields.sort();
        let mut known: Vec<String> = KNOWN_KEYS.iter().map(|s| s.to_string()).collect();
        known.sort();
        assert_eq!(fields, known, "format::KNOWN_KEYS and the fields of model::Model differ: update KNOWN_KEYS (and decide whether the new field needs a `needs` entry)");
    }

    #[test]
    fn plain_and_old_files_are_fine() {
        assert!(check(br#"{"next_id":1}"#).is_none());
        assert!(check(br#"{"format":1,"next_id":1,"needs":[]}"#).is_none());
        // what this build writes when it holds sheets, it reads
        assert!(check(br#"{"format":2,"needs":["sheets"],"next_id":1,"sheets":{},"sheet_items":{},"drafting":{},"title_blocks":{}}"#).is_none());
        // and projected texture coordinates (0.3.9)
        assert!(check(br#"{"format":2,"needs":["face_uv"],"next_id":1}"#).is_none());
        assert!(check(br#"{"format":2,"needs":["face_uv","nope"],"next_id":1}"#).is_some());
        // and painted-through placements
        assert!(check(br#"{"format":2,"needs":["face_tex"],"next_id":1}"#).is_none());
        // and UV seams and the lightmap set
        assert!(check(br#"{"format":2,"needs":["edge_seam","face_uv2"],"next_id":1}"#).is_none());
        assert!(check(b"not json").is_none());
        assert!(check(b"[1,2]").is_none());
    }

    #[test]
    fn newer_files_are_read_only_with_reasons() {
        let r = check(br#"{"format":3,"next_id":1}"#).unwrap();
        assert!(r.reasons[0].contains("format is 3"), "{r:?}");
        let r = check(br#"{"needs":["holograms"]}"#).unwrap();
        assert!(r.reasons[0].contains("holograms"), "{r:?}");
        let r = check(br#"{"holograms":{},"next_id":1}"#).unwrap();
        assert!(r.reasons[0].contains("\"holograms\""), "{r:?}");
        assert!(check(br#"{"format":"two"}"#).is_some());
        assert!(check(br#"{"format":-1}"#).is_some());
        assert!(check(br#"{"format":1.5}"#).is_some());
        assert!(check(br#"{"needs":"sheets"}"#).is_some());
    }
}
