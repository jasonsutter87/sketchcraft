//! Ready-made settings to put a model in: a suburban lot, a city block, a
//! park, desert, a waterfront, or a product studio. Each is built from
//! ordinary engine commands (so it's editable geometry), sized around what's
//! already in the model, and kept in one locked group tagged "Environment".
//! Front is -Y: the street side, the way doors and windows face.

use glam::DVec3;
use serde_json::{json, Value};

pub const KINDS: [(&str, &str); 6] = [
    ("suburban", "Suburban lot: lawn, fence, sidewalk, street, trees"),
    ("city", "City block: street, sidewalks, neighbours, streetlights, cars"),
    ("park", "Park: lawn, paths, a pond, trees, benches"),
    ("desert", "Desert: sand, rocks, cacti, palms"),
    ("waterfront", "Waterfront: boardwalk, beach, water, a pier"),
    ("studio", "Studio: a seamless white backdrop with softbox lights (for objects)"),
];

/// Small deterministic jitter (so the same model always gets the same trees).
fn hash(i: u32, salt: u32) -> f64 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x % 10_000) as f64 / 10_000.0
}

struct B {
    cmds: Vec<Value>,
    n: u32,
}

impl B {
    fn new() -> Self {
        B { cmds: Vec::new(), n: 0 }
    }
    fn push(&mut self, v: Value) {
        self.cmds.push(v);
    }
    fn name(&mut self) -> String {
        self.n += 1;
        format!("env{}", self.n)
    }
    /// A box from (x0,y0,z0) to (x1,y1,z1) in its own group, painted.
    fn slab(&mut self, label: &str, a: DVec3, b: DVec3, mat: &str) {
        let (lo, hi) = (a.min(b), a.max(b));
        if hi.x - lo.x < 1e-3 || hi.y - lo.y < 1e-3 {
            return;
        }
        let g = self.name();
        let f = self.name();
        self.push(json!({"cmd":"begin_group","name":label,"as":g}));
        self.push(json!({"cmd":"rect","from":[lo.x,lo.y,lo.z],"to":[hi.x,hi.y,lo.z],"as":f}));
        if hi.z - lo.z > 1e-3 {
            self.push(json!({"cmd":"pushpull","face":format!("@{f}"),"distance":hi.z - lo.z,"toward":[0,0,1]}));
        }
        self.push(json!({"cmd":"end_group"}));
        self.push(json!({"cmd":"paint","ids":[format!("@{g}")],"material":mat}));
    }
    /// A vertical cylinder (cactus, post).
    fn post(&mut self, label: &str, c: DVec3, r: f64, h: f64, mat: &str) {
        let g = self.name();
        let f = self.name();
        self.push(json!({"cmd":"begin_group","name":label,"as":g}));
        self.push(json!({"cmd":"circle","center":[c.x,c.y,c.z],"radius":r,"segments":12,"as":f}));
        self.push(json!({"cmd":"pushpull","face":format!("@{f}"),"distance":h,"toward":[0,0,1]}));
        self.push(json!({"cmd":"end_group"}));
        self.push(json!({"cmd":"paint","ids":[format!("@{g}")],"material":mat}));
    }
    fn asset(&mut self, key: &str, at: DVec3, rot: f64) {
        self.push(json!({"cmd":"insert","asset":key,"at":[at.x,at.y,at.z],"rotate":rot}));
    }
    fn material(&mut self, name: &str, c: [f32; 3]) {
        self.push(json!({"cmd":"material","name":name,"color":c}));
    }
}

/// The commands that build `kind` around a footprint (lo..hi, ground at lo.z).
pub fn build(kind: &str, lo: DVec3, hi: DVec3) -> anyhow::Result<Vec<Value>> {
    let gz = lo.z;
    let (w, d) = (hi.x - lo.x, hi.y - lo.y);
    let cx = (lo.x + hi.x) / 2.0;
    let mut b = B::new();
    let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
    match kind {
        "suburban" => {
            b.material("Lawn", [0.36, 0.55, 0.26]);
            b.material("Concrete walk", [0.78, 0.77, 0.74]);
            b.material("Asphalt", [0.17, 0.17, 0.18]);
            b.material("Road paint", [0.95, 0.82, 0.25]);
            b.material("Fence wood", [0.62, 0.48, 0.34]);
            let (x0, x1) = (lo.x - 6.0, hi.x + 6.0);
            let (front, back) = (lo.y - 9.0, hi.y + 9.0);
            b.slab("Lawn", p(x0, front, gz - 0.06), p(x1, back, gz - 0.02), "Lawn");
            b.slab("Sidewalk", p(x0 - 12.0, front - 1.6, gz - 0.06), p(x1 + 12.0, front, gz + 0.04), "Concrete walk");
            b.slab("Curb", p(x0 - 12.0, front - 1.8, gz - 0.12), p(x1 + 12.0, front - 1.6, gz + 0.04), "Concrete walk");
            b.slab("Street", p(x0 - 12.0, front - 10.0, gz - 0.14), p(x1 + 12.0, front - 1.8, gz - 0.1), "Asphalt");
            b.slab("Center line", p(x0 - 12.0, front - 6.0, gz - 0.1), p(x1 + 12.0, front - 5.85, gz - 0.095), "Road paint");
            // fence on the back and sides
            b.slab("Back fence", p(x0, back - 0.05, gz), p(x1, back, gz + 1.8), "Fence wood");
            b.slab("Side fence", p(x0, lo.y + d * 0.2, gz), p(x0 + 0.05, back, gz + 1.8), "Fence wood");
            b.slab("Side fence", p(x1 - 0.05, lo.y + d * 0.2, gz), p(x1, back, gz + 1.8), "Fence wood");
            for (i, (x, y)) in [(x0 + 2.2, back - 2.5), (x1 - 2.4, back - 3.0), (x0 + 2.0, front + 3.0), (x1 - 1.8, front + 2.2)].iter().enumerate() {
                b.asset("tree", p(*x, *y, gz), 37.0 * i as f64);
            }
            b.asset("streetlight", p(x0 - 2.0, front - 1.2, gz), 0.0);
            b.asset("hydrant", p(x1 + 3.0, front - 1.2, gz), 0.0);
            b.asset("car", p(cx + w * 0.3, front - 4.0, gz - 0.1), 0.0);
        }
        "city" => {
            b.material("Asphalt", [0.17, 0.17, 0.18]);
            b.material("Concrete walk", [0.74, 0.73, 0.70]);
            b.material("Road paint", [0.95, 0.95, 0.92]);
            let looks = [("Brick", [0.55, 0.30, 0.24]), ("Stucco", [0.86, 0.80, 0.70]), ("Concrete", [0.62, 0.62, 0.60]), ("Dark brick", [0.34, 0.24, 0.22])];
            for (n, c) in looks {
                b.material(n, c);
            }
            let front = lo.y - 0.5;
            let (x0, x1) = (lo.x - 40.0, hi.x + 40.0);
            b.slab("Sidewalk", p(x0, front - 3.5, gz - 0.06), p(x1, hi.y + 1.0, gz + 0.04), "Concrete walk");
            b.slab("Street", p(x0, front - 15.5, gz - 0.14), p(x1, front - 3.5, gz - 0.1), "Asphalt");
            for k in 0..14 {
                let x = x0 + 2.0 + k as f64 * 6.0;
                b.slab("Lane mark", p(x, front - 9.6, gz - 0.1), p(x + 3.0, front - 9.4, gz - 0.095), "Road paint");
            }
            b.slab("Far sidewalk", p(x0, front - 19.0, gz - 0.06), p(x1, front - 15.5, gz + 0.04), "Concrete walk");
            // neighbours left and right, a row across the street
            let mut x = lo.x - 1.5;
            for i in 0..3 {
                let bw = 8.0 + 6.0 * hash(i, 1);
                let h = 7.0 + 14.0 * hash(i, 2);
                b.slab("Neighbour", p(x - bw, lo.y, gz), p(x, lo.y + d.max(10.0), gz + h), looks[i as usize % 4].0);
                x -= bw + 1.0;
            }
            let mut x = hi.x + 1.5;
            for i in 3..6 {
                let bw = 8.0 + 6.0 * hash(i, 1);
                let h = 7.0 + 14.0 * hash(i, 2);
                b.slab("Neighbour", p(x, lo.y, gz), p(x + bw, lo.y + d.max(10.0), gz + h), looks[i as usize % 4].0);
                x += bw + 1.0;
            }
            let mut x = x0 + 4.0;
            for i in 6..14 {
                let bw = 7.0 + 7.0 * hash(i, 1);
                let h = 8.0 + 20.0 * hash(i, 2);
                b.slab("Across the street", p(x, front - 31.0, gz), p(x + bw, front - 19.5, gz + h), looks[i as usize % 4].0);
                x += bw + 1.0;
            }
            for k in 0..4 {
                b.asset("streetlight", p(x0 + 12.0 + k as f64 * 22.0, front - 3.0, gz), 0.0);
            }
            b.asset("car", p(cx - 6.0, front - 6.5, gz - 0.1), 0.0);
            b.asset("car", p(cx + 9.0, front - 12.5, gz - 0.1), 180.0);
            b.asset("bench", p(cx + w / 2.0 + 3.0, front - 2.0, gz), 0.0);
            b.asset("hydrant", p(cx - w / 2.0 - 2.0, front - 2.6, gz), 0.0);
        }
        "park" => {
            b.material("Lawn", [0.38, 0.58, 0.27]);
            b.material("Gravel path", [0.80, 0.74, 0.62]);
            b.material("Pond water", [0.22, 0.42, 0.55]);
            let r = (w.max(d) / 2.0 + 18.0).max(25.0);
            let (x0, y0, x1, y1) = (cx - r, lo.y - r * 0.6, cx + r, hi.y + r * 0.6);
            b.slab("Park lawn", p(x0, y0, gz - 0.06), p(x1, y1, gz - 0.02), "Lawn");
            b.slab("Path", p(x0, lo.y - 4.0, gz - 0.02), p(x1, lo.y - 2.2, gz), "Gravel path");
            b.slab("Path", p(lo.x - 4.0, y0, gz - 0.02), p(lo.x - 2.2, y1, gz), "Gravel path");
            b.slab("Pond", p(hi.x + 6.0, hi.y + 4.0, gz - 0.05), p(hi.x + 18.0, hi.y + 13.0, gz - 0.015), "Pond water");
            for i in 0..14 {
                let (u, v) = (hash(i, 7), hash(i, 9));
                let (x, y) = (x0 + 3.0 + u * (x1 - x0 - 6.0), y0 + 3.0 + v * (y1 - y0 - 6.0));
                // keep clear of the model, the paths and the pond
                let clear = (x < lo.x - 6.0 || x > hi.x + 3.0 || y < lo.y - 6.0 || y > hi.y + 3.0) && !(x > hi.x + 4.0 && y > hi.y + 2.0 && x < hi.x + 20.0 && y < hi.y + 15.0);
                if clear {
                    b.asset("tree", p(x, y, gz), 360.0 * hash(i, 3));
                }
            }
            b.asset("bench", p(cx - 3.0, lo.y - 5.0, gz), 0.0);
            b.asset("bench", p(cx + 4.0, lo.y - 5.0, gz), 0.0);
            b.asset("streetlight", p(lo.x - 5.0, lo.y - 5.0, gz), 0.0);
        }
        "desert" => {
            b.material("Sand", [0.86, 0.72, 0.52]);
            b.material("Desert rock", [0.62, 0.45, 0.34]);
            b.material("Cactus green", [0.30, 0.48, 0.30]);
            let r = (w.max(d) / 2.0 + 30.0).max(40.0);
            b.slab("Sand", p(cx - r, lo.y - r, gz - 0.06), p(cx + r, hi.y + r, gz - 0.02), "Sand");
            for i in 0..18 {
                let (u, v) = (hash(i, 11), hash(i, 13));
                let (x, y) = (cx - r + 4.0 + u * (2.0 * r - 8.0), lo.y - r + 4.0 + v * (d + 2.0 * r - 8.0));
                if x > lo.x - 4.0 && x < hi.x + 4.0 && y > lo.y - 4.0 && y < hi.y + 4.0 {
                    continue;
                }
                match i % 3 {
                    0 => {
                        let s = 0.4 + 1.4 * hash(i, 5);
                        b.slab("Rock", p(x, y, gz - 0.05), p(x + s * 1.3, y + s, gz + s * 0.7), "Desert rock");
                    }
                    1 => {
                        let h = 1.2 + 2.5 * hash(i, 6);
                        b.post("Cactus", p(x, y, gz), 0.22, h, "Cactus green");
                        b.post("Cactus arm", p(x + 0.45, y, gz + h * 0.45), 0.13, h * 0.35, "Cactus green");
                    }
                    _ => b.asset("palm", p(x, y, gz), 360.0 * hash(i, 4)),
                }
            }
        }
        "waterfront" => {
            b.material("Sand", [0.88, 0.80, 0.62]);
            b.material("Boardwalk", [0.55, 0.40, 0.27]);
            b.material("Sea water", [0.16, 0.36, 0.48]);
            let (x0, x1) = (lo.x - 30.0, hi.x + 30.0);
            b.slab("Lawn edge", p(x0, lo.y - 1.0, gz - 0.06), p(x1, hi.y + 12.0, gz - 0.02), "Sand");
            b.slab("Boardwalk", p(x0, lo.y - 5.0, gz - 0.02), p(x1, lo.y - 1.0, gz + 0.12), "Boardwalk");
            b.slab("Beach", p(x0, lo.y - 14.0, gz - 0.5), p(x1, lo.y - 5.0, gz - 0.06), "Sand");
            b.slab("Water", p(x0 - 10.0, lo.y - 55.0, gz - 0.6), p(x1 + 10.0, lo.y - 14.0, gz - 0.55), "Sea water");
            b.slab("Pier", p(cx - 1.5, lo.y - 40.0, gz - 0.02), p(cx + 1.5, lo.y - 5.0, gz + 0.12), "Boardwalk");
            for k in 0..8 {
                let y = lo.y - 8.0 - k as f64 * 4.4;
                b.post("Pier post", p(cx - 1.4, y, gz - 0.6), 0.12, 0.7, "Boardwalk");
                b.post("Pier post", p(cx + 1.4, y, gz - 0.6), 0.12, 0.7, "Boardwalk");
            }
            for k in 0..6 {
                b.asset("palm", p(x0 + 6.0 + k as f64 * ((x1 - x0 - 12.0) / 5.0), lo.y - 3.0 - 6.0 * (k % 2) as f64, gz), 50.0 * k as f64);
            }
            b.asset("bench", p(cx - 6.0, lo.y - 3.0, gz + 0.12), 0.0);
            b.asset("bench", p(cx + 6.0, lo.y - 3.0, gz + 0.12), 0.0);
        }
        "studio" => {
            b.material("Studio sweep", [0.93, 0.93, 0.92]);
            b.material("Softbox light", [1.0, 0.98, 0.94]);
            // a seamless sweep: floor curving up into the back wall
            let size = w.max(d).max(0.5);
            let (x0, x1) = (cx - size * 2.2 - 1.0, cx + size * 2.2 + 1.0);
            let (yf, yw, r, h) = (lo.y - size * 2.0 - 1.0, hi.y + size * 1.2 + 0.6, size * 0.9 + 0.3, size * 2.4 + 1.5);
            let t = 0.04;
            let mut prof = vec![[yf, gz - t], [yw - r, gz - t]];
            for k in 1..=10 {
                let a = std::f64::consts::FRAC_PI_2 * k as f64 / 10.0;
                prof.push([yw - r + (r + t) * a.sin(), gz + r - (r + t) * a.cos()]);
            }
            prof.push([yw + t, gz + h]);
            prof.push([yw, gz + h]);
            for k in (0..=10).rev() {
                let a = std::f64::consts::FRAC_PI_2 * k as f64 / 10.0;
                prof.push([yw - r + r * a.sin(), gz + r - r * a.cos()]);
            }
            prof.push([yf, gz]);
            let pts: Vec<Value> = prof.iter().map(|q| json!([x0, q[0], q[1]])).collect();
            let g = b.name();
            let f = b.name();
            b.push(json!({"cmd":"begin_group","name":"Studio sweep","as":g}));
            b.push(json!({"cmd":"polygon","points":pts,"as":f}));
            b.push(json!({"cmd":"pushpull","face":format!("@{f}"),"distance":x1 - x0,"toward":[1,0,0]}));
            b.push(json!({"cmd":"end_group"}));
            b.push(json!({"cmd":"paint","ids":[format!("@{g}")],"material":"Studio sweep"}));
            // softboxes at the sides, facing in (out of a front or 3/4 camera's way),
            // and one overhead: key left and high, fill right and lower
            let s = size * 0.9 + 0.4;
            let cy = (lo.y + hi.y) / 2.0 - size * 0.4;
            let off = size * 1.5 + 0.5;
            b.slab("Key softbox light", p(cx - off - 0.05, cy - s / 2.0, gz + size * 0.5), p(cx - off, cy + s / 2.0, gz + size * 0.5 + s), "Softbox light");
            b.slab("Fill softbox light", p(cx + off, cy - s * 0.4, gz + size * 0.2), p(cx + off + 0.05, cy + s * 0.4, gz + size * 0.2 + s * 0.8), "Softbox light");
            b.slab("Top softbox light", p(cx - s * 0.6, lo.y + d / 2.0 - s * 0.6, gz + size * 2.0 + 0.6), p(cx + s * 0.6, lo.y + d / 2.0 + s * 0.6, gz + size * 2.0 + 0.65), "Softbox light");
        }
        other => anyhow::bail!("no environment {other:?}: use {}", KINDS.iter().map(|k| k.0).collect::<Vec<_>>().join(", ")),
    }
    Ok(b.cmds)
}

/// The model without its environment (drawings and floor finding are about
/// the building, not the lawn around it). Borrowed as is when there's none.
pub fn without(m: &crate::model::Model) -> std::borrow::Cow<'_, crate::model::Model> {
    let env: Vec<u32> = m.groups.iter().filter(|(_, g)| g.parent.is_none() && g.name.starts_with("Environment")).map(|(&k, _)| k).collect();
    if env.is_empty() {
        return std::borrow::Cow::Borrowed(m);
    }
    let mut c = m.clone();
    for g in &env {
        c.groups.get_mut(g).unwrap().locked = false;
    }
    let _ = c.delete(&env);
    std::borrow::Cow::Owned(c)
}
