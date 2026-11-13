//! Small geometry helpers: plane normals, 2D projection, point-in-polygon,
//! triangulation. Units are meters, Z is up.

use glam::{DVec2, DVec3};

/// Unnormalized Newell normal: direction follows the right-hand rule around
/// the loop, length is twice the loop's area.
pub fn newell_raw(pts: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

pub fn newell(pts: &[DVec3]) -> DVec3 {
    newell_raw(pts).normalize_or_zero()
}

pub fn area(pts: &[DVec3]) -> f64 {
    newell_raw(pts).length() * 0.5
}

pub fn centroid(pts: &[DVec3]) -> DVec3 {
    pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64
}

/// Right-handed in-plane axes (u, v) for a unit normal: u x v = n.
/// Projecting with these keeps a loop that is CCW about n CCW in 2D.
pub fn basis(n: DVec3) -> (DVec3, DVec3) {
    let u = if n.z.abs() < 0.9 { DVec3::Z.cross(n) } else { DVec3::X.cross(n) }.normalize();
    (u, n.cross(u))
}

pub fn to2d(pts: &[DVec3], u: DVec3, v: DVec3) -> Vec<DVec2> {
    pts.iter().map(|p| DVec2::new(p.dot(u), p.dot(v))).collect()
}

/// Even-odd ray cast. Points on the boundary count as outside (good enough
/// for "is this new loop strictly inside that face").
pub fn inside(p: DVec2, poly: &[DVec2]) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c && !on_boundary(p, poly)
}

pub fn inside_or_on(p: DVec2, poly: &[DVec2]) -> bool {
    inside(p, poly) || on_boundary(p, poly)
}

/// Shoelace area: positive when the 2D loop is counter-clockwise.
pub fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>() * 0.5
}

pub fn on_boundary(p: DVec2, poly: &[DVec2]) -> bool {
    let n = poly.len();
    (0..n).any(|i| {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        (a + ab * t).distance(p) < 1e-7
    })
}

/// Triangulate a planar polygon with holes. Triangles come out wound
/// counter-clockwise about `n` (so their normals agree with the face).
pub fn triangulate(outer: &[DVec3], holes: &[Vec<DVec3>], n: DVec3) -> Vec<[DVec3; 3]> {
    let (u, v) = basis(n);
    let mut all: Vec<DVec3> = outer.to_vec();
    let mut hole_idx = Vec::new();
    for h in holes {
        hole_idx.push(all.len());
        all.extend_from_slice(h);
    }
    let flat: Vec<f64> = to2d(&all, u, v).iter().flat_map(|p| [p.x, p.y]).collect();
    let idx = match earcutr::earcut(&flat, &hole_idx, 2) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    idx.chunks_exact(3)
        .map(|t| {
            let (a, b, c) = (all[t[0]], all[t[1]], all[t[2]]);
            if (b - a).cross(c - a).dot(n) < 0.0 { [a, c, b] } else { [a, b, c] }
        })
        .collect()
}
