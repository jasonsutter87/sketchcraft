//! IFC4 export (the open BIM format Revit, ArchiCAD and others read).
//! Project > Site > Building > Storey; every group and component placement
//! is an element of its classified type (the Classifier), or a type guessed
//! from its name, or a generic proxy. Geometry is IfcTriangulatedFaceSet in
//! meters. The site carries the geo-location when the model has one.

use crate::geom;
use crate::model::Model;
use anyhow::Result;
use std::fmt::Write as _;

pub const CLASSES: [&str; 14] = [
    "IfcWall", "IfcSlab", "IfcRoof", "IfcDoor", "IfcWindow", "IfcColumn", "IfcBeam", "IfcStair", "IfcRailing", "IfcCovering", "IfcFurniture", "IfcPlate", "IfcMember", "IfcBuildingElementProxy",
];

/// A type from the name when nothing was classified ("Wall", "Gable roof", ...).
pub fn guess(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    let has = |w: &str| n.contains(w);
    if has("wall") {
        "IfcWall"
    } else if has("roof") {
        "IfcRoof"
    } else if has("stair") || has("step") {
        "IfcStair"
    } else if has("column") || has("post") || has("pillar") {
        "IfcColumn"
    } else if has("door") || has("garage") {
        "IfcDoor"
    } else if has("window") {
        "IfcWindow"
    } else if has("slab") || has("floor") || has("foundation") || has("deck") {
        "IfcSlab"
    } else if has("beam") {
        "IfcBeam"
    } else if has("rail") || has("fence") {
        "IfcRailing"
    } else {
        "IfcBuildingElementProxy"
    }
}

/// IFC's 22-character base-64 GlobalId, made from a number (stable per export).
fn guid(n: u64) -> String {
    const CH: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
    // spread the number over 128 bits with a fixed mix so ids look unrelated
    let mut x = (n as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835) ^ 0x5348_4554_4348_4352_4146_5400_0000_0001;
    let mut s = [0u8; 22];
    for c in s.iter_mut().rev() {
        *c = CH[(x & 63) as usize];
        x >>= 6;
    }
    s[0] = CH[(s[0] as usize) % 4]; // first char carries only 2 bits
    String::from_utf8(s.to_vec()).unwrap()
}

fn txt(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).collect::<String>().replace('\\', "\\\\").replace('\'', "''")
}

/// Degrees -> IFC's (degrees, minutes, seconds, millionths).
fn dms(v: f64) -> String {
    let neg = v < 0.0;
    let a = v.abs();
    let d = a.floor();
    let m = ((a - d) * 60.0).floor();
    let s = ((a - d) * 3600.0 - m * 60.0).floor();
    let u = (((a - d) * 3600.0 - m * 60.0 - s) * 1e6).round();
    let sg = if neg { -1.0 } else { 1.0 };
    format!("({},{},{},{})", sg * d, sg * m, sg * s, sg * u)
}

pub fn write_ifc(m: &Model, path: &str, project: &str) -> Result<usize> {
    let mut lines: Vec<String> = Vec::new();
    let id = |line: String, lines: &mut Vec<String>| -> usize {
        lines.push(line);
        lines.len()
    };
    let mut g = 0u64;
    let mut gid = || {
        g += 1;
        guid(g)
    };
    let owner = id("IFCPERSON($,'SketchCraft',$,$,$,$,$,$)".into(), &mut lines);
    let org = id("IFCORGANIZATION($,'SketchCraft',$,$,$)".into(), &mut lines);
    let po = id(format!("IFCPERSONANDORGANIZATION(#{owner},#{org},$)"), &mut lines);
    let app = id(format!("IFCAPPLICATION(#{org},'0.2','SketchCraft','SketchCraft')"), &mut lines);
    // made now; IFC4 wants a modified date whenever a change action is given
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let hist = id(format!("IFCOWNERHISTORY(#{po},#{app},$,.ADDED.,{now},#{po},#{app},{now})"), &mut lines);
    let origin = id("IFCCARTESIANPOINT((0.,0.,0.))".into(), &mut lines);
    let zdir = id("IFCDIRECTION((0.,0.,1.))".into(), &mut lines);
    let xdir = id("IFCDIRECTION((1.,0.,0.))".into(), &mut lines);
    let place = id(format!("IFCAXIS2PLACEMENT3D(#{origin},#{zdir},#{xdir})"), &mut lines);
    let north = id("IFCDIRECTION((0.,1.))".into(), &mut lines);
    let ctx = id(format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{place},#{north})"), &mut lines);
    let body = id(format!("IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{ctx},$,.MODEL_VIEW.,$)"), &mut lines);
    let ul = id("IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.)".into(), &mut lines);
    let ua = id("IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.)".into(), &mut lines);
    let uv = id("IFCSIUNIT(*,.VOLUMEUNIT.,$,.CUBIC_METRE.)".into(), &mut lines);
    let ur = id("IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.)".into(), &mut lines);
    let units = id(format!("IFCUNITASSIGNMENT((#{ul},#{ua},#{uv},#{ur}))"), &mut lines);
    let proj = id(format!("IFCPROJECT('{}',#{hist},'{}',$,$,$,$,(#{ctx}),#{units})", gid(), txt(project)), &mut lines);
    let lp = |rel: Option<usize>, lines: &mut Vec<String>| -> usize {
        lines.push(format!("IFCLOCALPLACEMENT({},#{place})", rel.map(|r| format!("#{r}")).unwrap_or("$".into())));
        lines.len()
    };
    let site_lp = lp(None, &mut lines);
    let (lat, lon, elev) = m.location.as_ref().map(|l| (dms(l.lat), dms(l.lon), format!("{:.3}", l.elevation))).unwrap_or(("$".into(), "$".into(), "$".into()));
    let site = id(format!("IFCSITE('{}',#{hist},'Site',$,$,#{site_lp},$,$,.ELEMENT.,{lat},{lon},{elev},$,$)", gid()), &mut lines);
    let bld_lp = lp(Some(site_lp), &mut lines);
    let bld = id(format!("IFCBUILDING('{}',#{hist},'Building',$,$,#{bld_lp},$,$,.ELEMENT.,$,$,$)", gid()), &mut lines);
    let st_lp = lp(Some(bld_lp), &mut lines);
    let storey = id(format!("IFCBUILDINGSTOREY('{}',#{hist},'Level 0',$,$,#{st_lp},$,$,.ELEMENT.,0.)", gid()), &mut lines);
    let r1 = gid();
    id(format!("IFCRELAGGREGATES('{r1}',#{hist},$,$,#{proj},(#{site}))"), &mut lines);
    let r2 = gid();
    id(format!("IFCRELAGGREGATES('{r2}',#{hist},$,$,#{site},(#{bld}))"), &mut lines);
    let r3 = gid();
    id(format!("IFCRELAGGREGATES('{r3}',#{hist},$,$,#{bld},(#{storey}))"), &mut lines);

    // elements: groups and placements, with their faces in world meters
    let faces = m.world_faces();
    let mut things: Vec<(u32, String, bool)> = m.groups.iter().map(|(k, gr)| (*k, gr.name.clone(), true)).collect();
    things.extend(m.instances.iter().map(|(k, i)| (*k, i.name.clone(), false)));
    let mut elements = Vec::new();
    for (eid, name, is_group) in things {
        let mine: Vec<_> = faces.iter().filter(|f| if is_group { f.group == Some(eid) && f.face.is_some() } else { f.owner == eid }).collect();
        if mine.is_empty() {
            continue;
        }
        let mut pts: Vec<[f64; 3]> = Vec::new();
        let mut tris: Vec<[usize; 3]> = Vec::new();
        for f in &mine {
            for t in geom::triangulate(&f.outer, &f.holes, f.normal) {
                let b = pts.len();
                pts.extend(t.iter().map(|p| p.to_array()));
                tris.push([b + 1, b + 2, b + 3]);
            }
        }
        let mut pl = String::from("IFCCARTESIANPOINTLIST3D((");
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                pl.push(',');
            }
            write!(pl, "({:.6},{:.6},{:.6})", p[0], p[1], p[2])?;
        }
        pl += "),$)";
        let plist = id(pl, &mut lines);
        let idx = tris.iter().map(|t| format!("({},{},{})", t[0], t[1], t[2])).collect::<Vec<_>>().join(",");
        let tfs = id(format!("IFCTRIANGULATEDFACESET(#{plist},$,.T.,({idx}),$)"), &mut lines);
        let rep = id(format!("IFCSHAPEREPRESENTATION(#{body},'Body','Tessellation',(#{tfs}))"), &mut lines);
        let shape = id(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"), &mut lines);
        let elp = lp(Some(st_lp), &mut lines);
        let class = m.classes.get(&eid).map(|s| s.as_str()).unwrap_or_else(|| guess(&name));
        let class = if CLASSES.contains(&class) { class } else { "IfcBuildingElementProxy" };
        let up = class.to_ascii_uppercase();
        // each type's attribute list after the common 8 (predefined type etc.)
        let tail = match class {
            // OverallHeight, OverallWidth, PredefinedType, OperationType / PartitioningType, UserDefined...
            "IfcDoor" | "IfcWindow" => "$,$,$,$,$",
            _ => "$", // PredefinedType
        };
        let e = id(format!("{up}('{}',#{hist},'{}',$,$,#{elp},#{shape},'{eid}',{tail})", gid(), txt(&name)), &mut lines);
        elements.push(e);
    }
    if !elements.is_empty() {
        let list = elements.iter().map(|e| format!("#{e}")).collect::<Vec<_>>().join(",");
        let r = gid();
        id(format!("IFCRELCONTAINEDINSPATIALSTRUCTURE('{r}',#{hist},$,$,({list}),#{storey})"), &mut lines);
    }
    let mut out = String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');\n");
    writeln!(out, "FILE_NAME('{}','',(''),(''),'SketchCraft','SketchCraft','');", txt(&std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()))?;
    out += "FILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n";
    for (i, l) in lines.iter().enumerate() {
        writeln!(out, "#{}={};", i + 1, l)?;
    }
    out += "ENDSEC;\nEND-ISO-10303-21;\n";
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, out)?;
    Ok(elements.len())
}
