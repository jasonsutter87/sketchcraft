//! Modeling from several matched photos. Each Match Photo camera knows its
//! direction and lens, and sits on the line through the world origin (the
//! corner marked as origin in every photo); only how far out it sits (the
//! scale) is a guess. Mark the same corner in two or more photos and this
//! solves, together and in one linear least-squares step:
//!
//! - every marked corner's 3D position, and
//! - every photo's distance along its line (one is held fixed, or the scale
//!   comes from a length you know),
//!
//! from "the corner minus the camera lies along the ray through the mark":
//! (I - r rᵀ)(P - d·e) = 0 for each mark (r: the mark's ray, e: the unit
//! direction from the origin out to the camera, d: its distance).

use crate::model::{Model, View};
use anyhow::{bail, Result};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A corner marked in photos: view name -> where (photo-centered, y up,
/// in units of half the photo's height: the corners of a 4:3 photo are at
/// (±1.333, ±1)).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PhotoPoint {
    #[serde(default)]
    pub name: String,
    pub marks: BTreeMap<String, [f64; 2]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Solved {
    /// each point's position (None: marked in fewer than two solved photos)
    pub points: Vec<Option<[f64; 3]>>,
    /// how far each mark's ray misses its point, meters (same order as the marks)
    pub misses: Vec<Vec<(String, f64)>>,
    /// the photos' solved distances from the origin
    pub distances: BTreeMap<String, f64>,
}

/// The camera's frame for a saved view: forward, right, up (z-up world).
fn frame(v: &View) -> (DVec3, DVec3, DVec3, DVec3) {
    let eye = DVec3::from(v.eye);
    let fwd = (DVec3::from(v.target) - eye).normalize_or_zero();
    let up0 = if fwd.cross(DVec3::Z).length() < 1e-6 { DVec3::Y } else { DVec3::Z };
    let right = fwd.cross(up0).normalize();
    let up = right.cross(fwd);
    (eye, fwd, right, up)
}

/// The ray (unit direction) through a mark on a view's photo.
pub fn ray(v: &View, at: [f64; 2]) -> DVec3 {
    let (_, fwd, right, up) = frame(v);
    let t = (v.fov.to_radians() / 2.0).tan();
    (fwd + right * (at[0] * t) + up * (at[1] * t)).normalize()
}

/// Where a 3D point lands on a view's photo (same units as marks), if in front.
pub fn project(v: &View, p: DVec3) -> Option<[f64; 2]> {
    let (eye, fwd, right, up) = frame(v);
    let d = p - eye;
    let z = d.dot(fwd);
    if z <= 1e-6 {
        return None;
    }
    let t = (v.fov.to_radians() / 2.0).tan();
    Some([d.dot(right) / z / t, d.dot(up) / z / t])
}

/// Solve points and photo distances. `fixed`: the photo whose distance is
/// kept (default: the first photo used). `scale`: points a and b are this
/// many meters apart (sets the scale instead of the fixed photo).
pub fn solve(m: &Model, points: &[PhotoPoint], fixed: Option<&str>, scale: Option<(usize, usize, f64)>) -> Result<Solved> {
    // the photos in play, and their unknown distances
    let photo = |name: &str| m.views.iter().find(|v| v.name == name && v.photo.is_some());
    let mut views: Vec<&View> = Vec::new();
    for p in points {
        for name in p.marks.keys() {
            let Some(v) = photo(name) else { bail!("no matched photo named {name:?}") };
            if !views.iter().any(|x| x.name == v.name) {
                views.push(v);
            }
        }
    }
    // only points seen in two or more photos can be placed
    let usable: Vec<usize> = (0..points.len()).filter(|&i| points[i].marks.len() >= 2).collect();
    if usable.is_empty() {
        bail!("mark at least one corner in two photos");
    }
    let anchor = match fixed {
        Some(f) => views.iter().position(|v| v.name == f).ok_or_else(|| anyhow::anyhow!("{f:?} has no marks"))?,
        None => 0,
    };
    let dir = |v: &View| DVec3::from(v.eye).normalize_or_zero();
    let dist0 = |v: &View| DVec3::from(v.eye).length();
    // unknowns: [P for each usable point (3)] [d for each view except the anchor]
    let np = usable.len();
    let col_d = |vi: usize| if vi == anchor { None } else { Some(3 * np + if vi < anchor { vi } else { vi - 1 }) };
    let n = 3 * np + views.len() - 1;
    let mut ata = vec![vec![0.0f64; n]; n];
    let mut atb = vec![0.0f64; n];
    for (pi, &k) in usable.iter().enumerate() {
        for (name, at) in &points[k].marks {
            let vi = views.iter().position(|v| &v.name == name).unwrap();
            let v = views[vi];
            let r = ray(v, *at);
            let e = dir(v);
            // rows: (I - r rᵀ) P - (I - r rᵀ) e d = (anchor: (I - r rᵀ) e d0)
            for row in 0..3 {
                let mut coef: Vec<(usize, f64)> = Vec::with_capacity(4);
                for c in 0..3 {
                    let pr = (if row == c { 1.0 } else { 0.0 }) - r[row] * r[c];
                    coef.push((3 * pi + c, pr));
                }
                let pe = e[row] - r[row] * r.dot(e);
                let mut rhs = 0.0;
                match col_d(vi) {
                    Some(c) => coef.push((c, -pe)),
                    None => rhs = pe * dist0(v),
                }
                for &(i, a) in &coef {
                    atb[i] += a * rhs;
                    for &(j, b) in &coef {
                        ata[i][j] += a * b;
                    }
                }
            }
        }
    }
    // a whisper of damping toward the current distances keeps photos that
    // share no points with the rest where they were
    for (vi, v) in views.iter().enumerate() {
        if let Some(c) = col_d(vi) {
            let w = 1e-9;
            ata[c][c] += w;
            atb[c] += w * dist0(v);
        }
    }
    let x = gauss(ata, atb).ok_or_else(|| anyhow::anyhow!("the marks don't pin the points down: mark each corner in photos taken from different places"))?;
    let mut pts: Vec<Option<DVec3>> = vec![None; points.len()];
    for (pi, &k) in usable.iter().enumerate() {
        pts[k] = Some(DVec3::new(x[3 * pi], x[3 * pi + 1], x[3 * pi + 2]));
    }
    let mut distances: BTreeMap<String, f64> = views.iter().enumerate().map(|(vi, v)| (v.name.clone(), col_d(vi).map(|c| x[c]).unwrap_or(dist0(v)))).collect();
    if distances.values().any(|d| *d <= 0.0) {
        bail!("a photo ended up behind the origin: check that the origin marker is on the same corner in every photo");
    }
    // a known length sets the scale (everything about the origin)
    if let Some((a, b, len)) = scale {
        let (Some(Some(pa)), Some(Some(pb))) = (pts.get(a), pts.get(b)) else { bail!("both points of the known length must be placed") };
        let now = pa.distance(*pb);
        if now < 1e-9 || len <= 0.0 {
            bail!("the known length needs two different points and a positive length");
        }
        let k = len / now;
        for p in pts.iter_mut().flatten() {
            *p *= k;
        }
        for d in distances.values_mut() {
            *d *= k;
        }
    }
    // how far each mark's ray (from its photo's solved position) misses its point
    let misses = points
        .iter()
        .zip(&pts)
        .map(|(p, sol)| {
            let Some(q) = sol else { return vec![] };
            p.marks
                .iter()
                .map(|(name, at)| {
                    let v = views.iter().find(|v| &v.name == name).unwrap();
                    let eye = dir(v) * distances[name];
                    let r = ray(v, *at);
                    let w = *q - eye;
                    (name.clone(), (w - r * w.dot(r)).length())
                })
                .collect()
        })
        .collect();
    Ok(Solved { points: pts.into_iter().map(|p| p.map(|q| q.to_array())).collect(), misses, distances })
}

/// Solve a small dense system (normal equations) by Gaussian elimination.
fn gauss(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for i in 0..n {
        let p = (i..n).max_by(|&x, &y| a[x][i].abs().total_cmp(&a[y][i].abs()))?;
        if a[p][i].abs() < 1e-12 {
            return None;
        }
        a.swap(i, p);
        b.swap(i, p);
        for r in i + 1..n {
            let f = a[r][i] / a[i][i];
            if f != 0.0 {
                for c in i..n {
                    a[r][c] -= f * a[i][c];
                }
                b[r] -= f * b[i];
            }
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|c| a[i][c] * x[c]).sum();
        x[i] = (b[i] - s) / a[i][i];
    }
    Some(x)
}

/// Square up: edges within `deg` of a red, green or blue axis are made exactly
/// parallel to it (their ends share the other two coordinates), so walls are
/// plumb, floors level and outlines close into flat faces. Returns how many
/// edges were squared.
pub fn square(points: &mut [Option<[f64; 3]>], edges: &[[usize; 2]], deg: f64) -> usize {
    let cos = deg.to_radians().cos();
    let n = points.len();
    // for each coordinate (x, y, z): which corners must share it
    let mut parent: Vec<[usize; 3]> = (0..n).map(|i| [i, i, i]).collect();
    fn find(p: &mut Vec<[usize; 3]>, c: usize, i: usize) -> usize {
        let mut r = i;
        while p[r][c] != r {
            r = p[r][c];
        }
        let mut j = i;
        while p[j][c] != r {
            let nx = p[j][c];
            p[j][c] = r;
            j = nx;
        }
        r
    }
    let mut squared = 0;
    for [a, b] in edges {
        let (Some(Some(pa)), Some(Some(pb))) = (points.get(*a).copied(), points.get(*b).copied()) else { continue };
        let d = DVec3::from(pb) - DVec3::from(pa);
        let l = d.length();
        if l < 1e-9 {
            continue;
        }
        if let Some(axis) = (0..3).find(|k| d[*k].abs() / l >= cos) {
            squared += 1;
            // along this axis: the ends share the other two coordinates
            for c in (0..3).filter(|c| *c != axis) {
                let (ra, rb) = (find(&mut parent, c, *a), find(&mut parent, c, *b));
                if ra != rb {
                    parent[ra][c] = rb;
                }
            }
        }
    }
    // each group of corners gets its average for that coordinate
    for c in 0..3 {
        let mut sum: std::collections::HashMap<usize, (f64, usize)> = Default::default();
        for i in 0..n {
            if let Some(p) = points[i] {
                let r = find(&mut parent, c, i);
                let e = sum.entry(r).or_insert((0.0, 0));
                e.0 += p[c];
                e.1 += 1;
            }
        }
        for i in 0..n {
            if let Some(p) = points[i].as_mut() {
                let r = find(&mut parent, c, i);
                let (s, k) = sum[&r];
                p[c] = s / k as f64;
            }
        }
    }
    squared
}
