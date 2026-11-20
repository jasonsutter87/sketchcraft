//! Projection mapping: put a material's picture on a selection with one of four projections (planar, box, cylinder,
//! sphere) and keep the result as the faces' own texture coordinates (`Face::uv`, one pair per corner).
//!
//! The math is plain functions of points, so it is tested on its own ([`Setup::uvs`]); [`Model::project_texture`] gathers
//! a selection, works out the frame and fit, and writes the coordinates. Design notes: docs/dev/projection-mapping.md.
//!
//! Conventions: (u, v) are in tiles (1 = one whole picture), v runs up the picture. "Fit" makes the picture cover the
//! selection once (once around for cylinder and sphere); without it the picture is as big as its tile size in metres
//! (the texture's width and height). `scale` is a size factor in both cases (2 = the picture twice as big).

use crate::geom;
use crate::model::{FaceUv, Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Planar,
    Box,
    Cylinder,
    Sphere,
}

impl Mode {
    pub fn parse(s: &str) -> Result<Mode> {
        Ok(match s {
            "planar" | "plane" => Mode::Planar,
            "box" | "triplanar" | "cube" => Mode::Box,
            "cylinder" | "cylindrical" => Mode::Cylinder,
            "sphere" | "spherical" => Mode::Sphere,
            other => bail!("unknown projection \"{other}\": use planar, box, cylinder or sphere (or clear)"),
        })
    }
}

/// What the person chose. Everything but the mode is optional: the engine fills in a sensible default from the selection.
#[derive(Clone, Debug)]
pub struct Projection {
    pub mode: Mode,
    /// planar: the picture's centre lands here (when not fitting); box: the same; cylinder and sphere: the axis or
    /// centre passes through here. Default: the middle of the selection's bounding box.
    pub origin: Option<DVec3>,
    /// planar: the direction the projector shines (default: against the biggest face's normal, so it lands on that
    /// face); box: the frame's up axis (default z); cylinder and sphere: the axis (default z, the vertical).
    pub axis: Option<DVec3>,
    /// size factor, 1 by default
    pub scale: f64,
    /// degrees: planar and box turn the picture in its plane; cylinder and sphere turn it around the axis (the seam moves)
    pub rotation: f64,
    /// cover the selection once (default for cylinder and sphere); false: use the picture's own tile size (default
    /// for planar and box)
    pub fit: Option<bool>,
}

impl Projection {
    pub fn new(mode: Mode) -> Projection {
        Projection { mode, origin: None, axis: None, scale: 1.0, rotation: 0.0, fit: None }
    }
}

/// A projection with every default filled in: ready to turn points into coordinates.
#[derive(Clone, Debug)]
pub struct Setup {
    pub mode: Mode,
    pub origin: DVec3,
    /// the frame: e[2] is the axis; e[0] x e[1] = e[2]
    pub e: [DVec3; 3],
    pub fit: bool,
    pub scale: f64,
    /// angle added around the axis (cylinder, sphere), radians
    pub spin: f64,
    /// the selection's extent along each frame axis, measured from `origin`
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    /// mean distance of the selection's side points from the axis (cylinder), or from the centre (sphere)
    pub radius: f64,
}

/// A frame whose third axis is `a` (unit), the other two turned by `deg` degrees about it.
pub fn frame(a: DVec3, deg: f64) -> [DVec3; 3] {
    // a sideways axis keeps the picture upright (its v runs up the world); a vertical one starts along +x
    let (b1, b2) = if a.z.abs() < 0.9 {
        let up = (DVec3::Z - a * a.z).normalize();
        (up.cross(a), up)
    } else {
        let x = (DVec3::X - a * a.x).normalize();
        (x, a.cross(x))
    };
    let r = deg.to_radians();
    let e1 = b1 * r.cos() + b2 * r.sin();
    let e2 = a.cross(e1);
    [e1, e2, a]
}

fn unit(v: DVec3, what: &str) -> Result<DVec3> {
    let n = v.normalize_or_zero();
    if !v.is_finite() || n == DVec3::ZERO {
        bail!("{what} can't be zero");
    }
    Ok(n)
}

impl Setup {
    /// Fill in the defaults for `rings` (every loop of every selected face, in world points) with the biggest face's
    /// normal `dominant`.
    pub fn new(p: &Projection, rings: &[&[DVec3]], dominant: DVec3) -> Result<Setup> {
        if !p.scale.is_finite() || p.scale < 1e-6 {
            bail!("scale must be at least 0.000001 (a size factor: 1 is the picture's own size)");
        }
        if !p.rotation.is_finite() {
            bail!("rotation must be a number");
        }
        let pts = || rings.iter().flat_map(|r| r.iter().copied());
        let (mut mn, mut mx) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
        for q in pts() {
            mn = mn.min(q);
            mx = mx.max(q);
        }
        if mn.x > mx.x {
            bail!("nothing to project onto");
        }
        let origin = p.origin.unwrap_or((mn + mx) * 0.5);
        let mut spin = 0.0;
        let mut axis = match (p.axis, p.mode) {
            (Some(a), _) => unit(a, "the axis")?,
            (None, Mode::Planar) => -dominant.normalize_or(DVec3::Z),
            (None, _) => DVec3::Z,
        };
        // planar: the picture faces the projector, so the frame's axis points back at it
        if p.mode == Mode::Planar {
            axis = -axis;
        }
        let e = match p.mode {
            Mode::Planar | Mode::Box => frame(axis, p.rotation),
            _ => {
                spin = p.rotation.to_radians();
                frame(axis, 0.0)
            }
        };
        let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
        let (mut rs, mut rn) = (0.0, 0usize);
        for q in pts() {
            let d = q - origin;
            for k in 0..3 {
                let c = d.dot(e[k]);
                lo[k] = lo[k].min(c);
                hi[k] = hi[k].max(c);
            }
            let r = match p.mode {
                Mode::Sphere => d.length(),
                _ => (d - e[2] * d.dot(e[2])).length(),
            };
            rs += r;
            rn += 1;
        }
        // a flat or zero-size selection would divide by nothing
        for k in 0..3 {
            if hi[k] - lo[k] < 1e-9 {
                hi[k] = lo[k] + 1e-9;
            }
        }
        let radius = (rs / rn.max(1) as f64).max(1e-9);
        let fit = p.fit.unwrap_or(matches!(p.mode, Mode::Cylinder | Mode::Sphere));
        Ok(Setup { mode: p.mode, origin, e, fit, scale: p.scale, spin, lo, hi, radius })
    }

    /// Texture coordinates for every corner of a face: `rings[0]` is the outer loop, the rest are holes; `normal` is the
    /// face's normal; `tile` is the picture's size in metres (used when not fitting).
    pub fn uvs(&self, rings: &[&[DVec3]], normal: DVec3, tile: (f64, f64)) -> Vec<Vec<[f64; 2]>> {
        let (tw, th) = (tile.0.max(1e-9) * self.scale, tile.1.max(1e-9) * self.scale);
        match self.mode {
            Mode::Planar => self.flat(rings, 2, tw, th),
            Mode::Box => {
                let n = [normal.dot(self.e[0]), normal.dot(self.e[1]), normal.dot(self.e[2])];
                // the axis the normal leans on most (ties: up, then the first)
                let k = if n[2].abs() >= n[0].abs() && n[2].abs() >= n[1].abs() {
                    2
                } else if n[0].abs() >= n[1].abs() {
                    0
                } else {
                    1
                };
                let k = if n[k] < 0.0 { k + 3 } else { k };
                self.flat(rings, k, tw, th)
            }
            Mode::Cylinder => {
                let cap = normal.dot(self.e[2]).abs() >= std::f64::consts::FRAC_1_SQRT_2;
                if cap {
                    let k = if normal.dot(self.e[2]) >= 0.0 { 2 } else { 5 };
                    // a cap is a disc: fit puts its radius across the picture
                    return self.cap(rings, k, tw, th);
                }
                self.wrap(rings, tw, th)
            }
            Mode::Sphere => self.wrap(rings, tw, th),
        }
    }

    /// A plane facing +e[k] (k 0-2) or -e[k-3] (k 3-5), the picture upright where there is an up (e3).
    /// Seen from outside, the picture reads left to right and is never mirrored.
    fn flat(&self, rings: &[&[DVec3]], k: usize, tw: f64, th: f64) -> Vec<Vec<[f64; 2]>> {
        // (axis index, sign) for u and v
        let (ua, us, va, vs): (usize, f64, usize, f64) = match k {
            0 => (1, 1.0, 2, 1.0),
            3 => (1, -1.0, 2, 1.0),
            1 => (0, -1.0, 2, 1.0),
            4 => (0, 1.0, 2, 1.0),
            2 => (0, 1.0, 1, 1.0),
            _ => (0, 1.0, 1, -1.0),
        };
        let range = |a: usize, s: f64| if s > 0.0 { (self.lo[a], self.hi[a]) } else { (-self.hi[a], -self.lo[a]) };
        let (ur, vr) = (range(ua, us), range(va, vs));
        rings
            .iter()
            .map(|r| {
                r.iter()
                    .map(|q| {
                        let d = *q - self.origin;
                        let (m, n) = (us * d.dot(self.e[ua]), vs * d.dot(self.e[va]));
                        if self.fit {
                            [(m - ur.0) / (ur.1 - ur.0) / self.scale, (n - vr.0) / (vr.1 - vr.0) / self.scale]
                        } else {
                            [0.5 + m / tw, 0.5 + n / th]
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// The end of a cylinder: planar on the cross-section, the picture upright to the cylinder's own frame.
    fn cap(&self, rings: &[&[DVec3]], k: usize, tw: f64, th: f64) -> Vec<Vec<[f64; 2]>> {
        let flip = if k == 5 { -1.0 } else { 1.0 };
        rings
            .iter()
            .map(|r| {
                r.iter()
                    .map(|q| {
                        let d = *q - self.origin;
                        let (x, y) = (d.dot(self.e[0]), flip * d.dot(self.e[1]));
                        if self.fit {
                            [(0.5 + x / (2.0 * self.radius)) / self.scale, (0.5 + y / (2.0 * self.radius)) / self.scale]
                        } else {
                            [0.5 + x / tw, 0.5 + y / th]
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// Around the axis (cylinder: height up it; sphere: latitude). The angle is unwrapped within the face, so a face
    /// that crosses the seam gets coordinates past 1 (or below 0) rather than a smear back across the picture.
    fn wrap(&self, rings: &[&[DVec3]], tw: f64, th: f64) -> Vec<Vec<[f64; 2]>> {
        let sphere = self.mode == Mode::Sphere;
        // per point: angle, the other coordinate, and whether it sits on the axis (angle undefined)
        let polar = |q: &DVec3| {
            let d = *q - self.origin;
            let (x, y, z) = (d.dot(self.e[0]), d.dot(self.e[1]), d.dot(self.e[2]));
            let radial = x.hypot(y);
            let on_axis = radial < 1e-9 * self.radius.max(1e-6);
            let theta = if on_axis { 0.0 } else { y.atan2(x) + self.spin };
            let up = if sphere { z.atan2(radial) } else { z };
            (theta, up, on_axis)
        };
        // the first off-axis corner of the outer loop sets which side of the seam the face is on
        let reference = rings.first().and_then(|r| r.iter().map(&polar).find(|p| !p.2)).map(|p| p.0).unwrap_or(0.0);
        let mut out: Vec<Vec<[f64; 2]>> = rings
            .iter()
            .map(|r| {
                let ps: Vec<(f64, f64, bool)> = r
                    .iter()
                    .map(|q| {
                        let (t, up, on) = polar(q);
                        let t = if on { t } else { reference + (t - reference + PI).rem_euclid(2.0 * PI) - PI };
                        (t, up, on)
                    })
                    .collect();
                // a pole sits at the average angle of its face, so a triangle fan doesn't shear
                let off: Vec<f64> = ps.iter().filter(|p| !p.2).map(|p| p.0).collect();
                let mean = if off.is_empty() { reference } else { off.iter().sum::<f64>() / off.len() as f64 };
                ps.iter()
                    .map(|&(t, up, on)| {
                        let t = if on { mean } else { t };
                        self.wrapped(t, up, sphere, tw, th)
                    })
                    .collect()
            })
            .collect();
        // keep the face's coordinates near 0..1 so a seam face doesn't carry a big whole-number offset
        let first = out.first().and_then(|r| r.first()).map(|p| p[0]).unwrap_or(0.0);
        let shift = (first / (1.0 / self.scale.max(1e-9))).floor() * (1.0 / self.scale.max(1e-9));
        if self.fit && shift != 0.0 {
            for r in &mut out {
                for p in r {
                    p[0] -= shift;
                }
            }
        }
        out
    }

    fn wrapped(&self, theta: f64, up: f64, sphere: bool, tw: f64, th: f64) -> [f64; 2] {
        if self.fit {
            let u = (theta + PI) / (2.0 * PI) / self.scale;
            let v = if sphere {
                (up + PI / 2.0) / PI / self.scale
            } else {
                (up - self.lo[2]) / (self.hi[2] - self.lo[2]) / self.scale
            };
            [u, v]
        } else {
            let u = 0.5 + theta * self.radius / tw;
            let v = 0.5 + if sphere { up * self.radius } else { up } / th;
            [u, v]
        }
    }
}

/// Picture sizes in metres by material name (width, height): what the tile-size mode measures against.
pub type Tiles = std::collections::HashMap<String, (f64, f64)>;

impl Model {
    /// Every textured material's picture size, here and in components.
    pub fn texture_tiles(&self) -> Tiles {
        let mut out = Tiles::new();
        for d in self.definitions.values() {
            for (n, m) in &d.model.materials {
                if let Some(t) = &m.texture {
                    out.insert(n.clone(), (t.width, t.height));
                }
            }
        }
        for (n, m) in &self.materials {
            if let Some(t) = &m.texture {
                out.insert(n.clone(), (t.width, t.height));
            }
        }
        out
    }

    /// Project a material's picture onto faces (or the faces of groups), painting them with `material` when given.
    /// Returns how many faces got coordinates. Nothing changes if it is refused.
    pub fn project_texture(&mut self, ids: &[Id], p: &Projection, material: Option<&str>) -> Result<usize> {
        let tiles = self.texture_tiles();
        self.project_texture_with(ids, p, material, &tiles)
    }

    /// As `project_texture`, with the picture sizes from elsewhere (a component's model doesn't hold the root's materials).
    pub fn project_texture_with(&mut self, ids: &[Id], p: &Projection, material: Option<&str>, tiles: &Tiles) -> Result<usize> {
        self.check_unlocked(ids)?;
        let faces = {
            let mut f = self.faces_of(ids)?;
            f.sort_unstable();
            f.dedup();
            f
        };
        if faces.is_empty() {
            bail!("select some faces (or a group) to project onto");
        }
        let rings: Vec<(Id, Vec<Vec<DVec3>>)> = faces.iter().map(|&id| (id, self.faces[&id].loops().map(|l| self.pts(l)).collect())).collect();
        let all: Vec<&[DVec3]> = rings.iter().flat_map(|(_, r)| r.iter().map(|v| v.as_slice())).collect();
        // the biggest face decides the planar default
        let dominant = rings
            .iter()
            .map(|(_, r)| geom::newell_raw(&r[0]))
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            .unwrap_or(DVec3::Z);
        let setup = Setup::new(p, &all, dominant)?;
        let mut done = Vec::with_capacity(rings.len());
        for (id, r) in &rings {
            let name = material.or(self.faces[id].material.as_deref());
            let tile = name.and_then(|m| tiles.get(m)).copied().unwrap_or((1.0, 1.0));
            let n = geom::newell(&r[0]);
            let refs: Vec<&[DVec3]> = r.iter().map(|v| v.as_slice()).collect();
            let mut uv = setup.uvs(&refs, n, tile).into_iter();
            let outer = crate::model::uv32_ring(&uv.next().unwrap_or_default())?;
            let holes = uv.map(|r| crate::model::uv32_ring(&r)).collect::<Result<_>>()?;
            done.push((*id, FaceUv { outer, holes }));
        }
        let n = done.len();
        for (id, uv) in done {
            let f = self.faces.get_mut(&id).unwrap();
            f.uv = Some(uv);
            if let Some(m) = material {
                f.material = Some(m.to_string());
            }
        }
        Ok(n)
    }

    /// Take projected coordinates off faces: the material's own placement is used again. Returns how many had them.
    pub fn clear_texture_projection(&mut self, ids: &[Id]) -> Result<usize> {
        self.check_unlocked(ids)?;
        let mut n = 0;
        for id in self.faces_of(ids)? {
            n += self.faces.get_mut(&id).unwrap().uv.take().is_some() as usize;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(pts: &[[f64; 3]]) -> Vec<DVec3> {
        pts.iter().map(|p| DVec3::new(p[0], p[1], p[2])).collect()
    }

    fn setup(mode: Mode, tweak: impl Fn(&mut Projection), rings: &[Vec<DVec3>], dominant: DVec3) -> Setup {
        let mut p = Projection::new(mode);
        tweak(&mut p);
        let refs: Vec<&[DVec3]> = rings.iter().map(|r| r.as_slice()).collect();
        Setup::new(&p, &refs, dominant).unwrap()
    }

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn planar_tile_size_centres_on_the_origin_and_follows_scale() {
        let quad = ring(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
        // a wall in the xz plane facing -y: the projector shines along +y
        let s = setup(Mode::Planar, |p| p.axis = Some(DVec3::Y), std::slice::from_ref(&quad), DVec3::NEG_Y);
        let uv = s.uvs(&[&quad], DVec3::NEG_Y, (1.0, 1.0));
        // seen from the front (-y): x runs right, z runs up; the middle of the wall is the middle of one tile
        let mid = (quad[0] + quad[2]) * 0.5;
        assert!(close([0.5, 0.5], [0.5 + (mid - s.origin).dot(s.e[0]), 0.5 + (mid - s.origin).dot(s.e[1])]));
        let w = (uv[0][1][0] - uv[0][0][0]).abs();
        assert!((w - 2.0).abs() < 1e-9, "2 m wide on a 1 m tile is 2 tiles: {w}");
        // double the scale: one tile now covers 2 m
        let s2 = setup(Mode::Planar, |p| { p.axis = Some(DVec3::Y); p.scale = 2.0 }, std::slice::from_ref(&quad), DVec3::NEG_Y);
        let uv2 = s2.uvs(&[&quad], DVec3::NEG_Y, (1.0, 1.0));
        assert!(((uv2[0][1][0] - uv2[0][0][0]).abs() - 1.0).abs() < 1e-9);
        // the picture is upright and not mirrored: up on the wall is up in the picture, and the front's right is u's way
        assert!(uv[0][2][1] > uv[0][1][1]);    }

    #[test]
    fn planar_default_direction_lands_on_the_biggest_face() {
        let floor = ring(&[[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0, 4.0, 0.0]]);
        let s = setup(Mode::Planar, |_| {}, std::slice::from_ref(&floor), DVec3::Z);
        // projector shines down (-z); the frame's axis points back up
        assert!((s.e[2] - DVec3::Z).length() < 1e-9);
        let uv = s.uvs(&[&floor], DVec3::Z, (1.0, 1.0));
        // a floor seen from above reads left to right along its own axes: u runs along x, v along y, in some order of sign
        let span_u = uv[0].iter().map(|p| p[0]).fold(f64::MIN, f64::max) - uv[0].iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        assert!((span_u - 4.0).abs() < 1e-9);
    }

    #[test]
    fn planar_fit_covers_the_selection_once_and_rotation_turns_it() {
        let quad = ring(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
        let s = setup(Mode::Planar, |p| { p.axis = Some(DVec3::Y); p.fit = Some(true) }, std::slice::from_ref(&quad), DVec3::NEG_Y);
        let uv = s.uvs(&[&quad], DVec3::NEG_Y, (1.0, 1.0));
        let (umin, umax) = uv[0].iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
        let (vmin, vmax) = uv[0].iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
        assert!(umin.abs() < 1e-9 && (umax - 1.0).abs() < 1e-9 && vmin.abs() < 1e-9 && (vmax - 1.0).abs() < 1e-9);
        // 90 degrees: the picture's up now runs along the wall's length
        let r = setup(Mode::Planar, |p| { p.axis = Some(DVec3::Y); p.fit = Some(true); p.rotation = 90.0 }, std::slice::from_ref(&quad), DVec3::NEG_Y);
        let ru = r.uvs(&[&quad], DVec3::NEG_Y, (1.0, 1.0));
        let dv = (ru[0][1][1] - ru[0][0][1]).abs();
        assert!((dv - 1.0).abs() < 1e-9, "v now follows the 2 m edge: {dv}");
    }

    #[test]
    fn box_uses_the_nearest_axis_plane_and_never_mirrors() {
        let top = ring(&[[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]]);
        let front = ring(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]]); // normal -y
        let right = ring(&[[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [1.0, 0.0, 1.0]]); // normal +x
        let s = setup(Mode::Box, |_| {}, &[top.clone(), front.clone(), right.clone()], DVec3::Z);
        let t = s.uvs(&[&top], DVec3::Z, (1.0, 1.0));
        let f = s.uvs(&[&front], DVec3::NEG_Y, (1.0, 1.0));
        let r = s.uvs(&[&right], DVec3::X, (1.0, 1.0));
        // top: u along +x, v along +y (looking down at it, not mirrored)
        assert!(t[0][1][0] > t[0][0][0] && t[0][3][1] > t[0][0][1]);
        // front (-y): seen from the front u runs along +x... which is -(e2 side)? it must read left to right: x grows with u
        assert!(f[0][1][0] > f[0][0][0] && f[0][2][1] > f[0][1][1]);
        // right (+x): seen from +x, left to right is +y
        assert!(r[0][1][0] > r[0][0][0] && r[0][2][1] > r[0][1][1]);
        // a 1 m face on a 1 m tile is exactly one tile
        assert!(((t[0][1][0] - t[0][0][0]).abs() - 1.0).abs() < 1e-9);
        // seams at box edges: the front's right edge and the right face's left edge are the same line but the pictures
        // don't have to agree there; each face is complete on its own
        assert!(f[0].iter().all(|p| p[0] >= -1e-9 && p[0] <= 1.0 + 1e-9) || !s.fit);
    }

    #[test]
    fn box_fit_gives_each_face_the_whole_picture() {
        let top = ring(&[[0.0, 0.0, 1.0], [3.0, 0.0, 1.0], [3.0, 2.0, 1.0], [0.0, 2.0, 1.0]]);
        let front = ring(&[[0.0, 0.0, 0.0], [3.0, 0.0, 0.0], [3.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
        let s = setup(Mode::Box, |p| p.fit = Some(true), &[top.clone(), front.clone()], DVec3::Z);
        for (r, n) in [(&top, DVec3::Z), (&front, DVec3::NEG_Y)] {
            let uv = s.uvs(&[r], n, (1.0, 1.0));
            let (umin, umax) = uv[0].iter().fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
            assert!(umin.abs() < 1e-9 && (umax - 1.0).abs() < 1e-9, "{umin} {umax}");
        }
    }

    /// a cylinder of radius 1 around z, `n` sides, from z=0 to 2: the side quad `i`
    fn drum(n: usize) -> (Vec<Vec<DVec3>>, Vec<DVec3>) {
        let at = |i: usize, z: f64| {
            let a = 2.0 * PI * (i % n) as f64 / n as f64;
            DVec3::new(a.cos(), a.sin(), z)
        };
        let sides = (0..n).map(|i| vec![at(i, 0.0), at(i + 1, 0.0), at(i + 1, 2.0), at(i, 2.0)]).collect();
        (sides, (0..n).map(|i| at(i, 2.0)).collect())
    }

    #[test]
    fn cylinder_wraps_once_around_and_is_continuous_but_for_one_seam() {
        let n = 16;
        let (sides, top) = drum(n);
        let mut rings = sides.clone();
        rings.push(top.clone());
        let s = setup(Mode::Cylinder, |_| {}, &rings, DVec3::Z);
        let uv: Vec<_> = sides.iter().map(|q| s.uvs(&[q], geom::newell(q), (1.0, 1.0)).remove(0)).collect();
        // heights: bottom 0, top 1
        for q in &uv {
            assert!(q[0][1].abs() < 1e-9 && (q[3][1] - 1.0).abs() < 1e-9);
        }
        // neighbours share a corner: its u matches (mod 1) everywhere; only the seam face crosses 1 -> 0
        let mut breaks = 0;
        for i in 0..n {
            let (a, b) = (&uv[i], &uv[(i + 1) % n]);
            let (d1, d2) = (a[1][0] - b[0][0], a[2][0] - b[3][0]);
            assert!((d1 - d1.round()).abs() < 1e-6 && (d2 - d2.round()).abs() < 1e-6, "face {i}: {d1} {d2}");
            if d1.abs() > 0.5 {
                breaks += 1;
            }
            // within a face, u grows by 1/n (never wraps back across the picture)
            assert!((a[1][0] - a[0][0] - 1.0 / n as f64).abs() < 1e-9, "face {i}");
        }
        assert_eq!(breaks, 1, "one seam");
        // the whole side covers exactly one picture
        let all: Vec<f64> = uv.iter().flatten().map(|p| p[0]).collect();
        let span = all.iter().cloned().fold(f64::MIN, f64::max) - all.iter().cloned().fold(f64::MAX, f64::min);
        assert!((span - 1.0).abs() < 0.05 + 1.0 / n as f64, "{span}");
    }

    #[test]
    fn cylinder_rotation_moves_the_seam_and_caps_are_planar() {
        let (sides, top) = drum(16);
        let mut rings = sides.clone();
        rings.push(top.clone());
        let a = setup(Mode::Cylinder, |_| {}, &rings, DVec3::Z);
        let b = setup(Mode::Cylinder, |p| p.rotation = 90.0, &rings, DVec3::Z);
        let ua = a.uvs(&[&sides[0]], geom::newell(&sides[0]), (1.0, 1.0))[0][0][0];
        let ub = b.uvs(&[&sides[0]], geom::newell(&sides[0]), (1.0, 1.0))[0][0][0];
        assert!(((ub - ua).rem_euclid(1.0) - 0.25).abs() < 1e-9);
        // the cap is the disc: the centre of the cap is the centre of the picture, the rim is a circle of radius 0.5
        let c = a.uvs(&[&top], DVec3::Z, (1.0, 1.0)).remove(0);
        for p in &c {
            let d = ((p[0] - 0.5).powi(2) + (p[1] - 0.5).powi(2)).sqrt();
            assert!((d - 0.5).abs() < 1e-9, "{d}");
        }
    }

    #[test]
    fn cylinder_tile_size_uses_arc_length() {
        let (sides, _) = drum(32);
        let s = setup(Mode::Cylinder, |p| p.fit = Some(false), &sides, DVec3::Z);
        let uv = s.uvs(&[&sides[3]], geom::newell(&sides[3]), (0.5, 0.5));
        // 32 faces of angle 2pi/32 on radius ~0.99 (mean of the corner distances) on a 0.5 m tile
        let arc = 2.0 * PI / 32.0 * s.radius / 0.5;
        assert!((uv[0][1][0] - uv[0][0][0] - arc).abs() < 1e-9);
        assert!((uv[0][3][1] - uv[0][0][1] - 2.0 / 0.5).abs() < 1e-9);
    }

    #[test]
    fn sphere_is_latitude_and_longitude_with_poles_at_the_average_angle() {
        // an octahedron-ish fan: a triangle with its tip at the north pole
        let r = 1.0;
        let tri = ring(&[[r, 0.0, 0.0], [0.0, r, 0.0], [0.0, 0.0, r]]);
        let other = ring(&[[-r, 0.0, 0.0], [0.0, -r, 0.0], [0.0, 0.0, -r]]);
        let s = setup(Mode::Sphere, |_| {}, &[tri.clone(), other.clone()], DVec3::Z);
        let uv = s.uvs(&[&tri], geom::newell(&tri), (1.0, 1.0)).remove(0);
        // equator is v = 0.5, the pole is v = 1
        assert!((uv[0][1] - 0.5).abs() < 1e-9 && (uv[2][1] - 1.0).abs() < 1e-9);
        // east (+x) is the middle of the picture, +y a quarter further round; the pole takes their average
        assert!((uv[0][0] - 0.5).abs() < 1e-9 && (uv[1][0] - 0.75).abs() < 1e-9 && (uv[2][0] - 0.625).abs() < 1e-9);
        let all = s.uvs(&[&other], geom::newell(&other), (1.0, 1.0)).remove(0);
        assert!(all.iter().all(|p| (-1e-9..=1.0 + 1e-9).contains(&p[0]) && (-1e-9..=1.0 + 1e-9).contains(&p[1])));
        assert!((all[2][1]).abs() < 1e-9, "south pole is v = 0");
    }

    #[test]
    fn a_face_across_the_seam_keeps_its_corners_together() {
        // the seam is at -x (angle pi): a quad straddling it
        let a = 2.0 * PI * 0.48;
        let b = 2.0 * PI * 0.52;
        let q = ring(&[[a.cos(), a.sin(), 0.0], [b.cos(), b.sin(), 0.0], [b.cos(), b.sin(), 1.0], [a.cos(), a.sin(), 1.0]]);
        let other = ring(&[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 1.0]]);
        let s = setup(Mode::Cylinder, |p| p.origin = Some(DVec3::ZERO), &[q.clone(), other], DVec3::Z);
        let uv = s.uvs(&[&q], geom::newell(&q), (1.0, 1.0)).remove(0);
        assert!((uv[1][0] - uv[0][0] - 0.04).abs() < 1e-9, "{uv:?}");
    }

    #[test]
    fn bad_settings_are_refused() {
        let q = ring(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]);
        let refs: Vec<&[DVec3]> = vec![q.as_slice()];
        let mut p = Projection::new(Mode::Box);
        p.scale = 0.0;
        assert!(Setup::new(&p, &refs, DVec3::Z).is_err());
        let mut p = Projection::new(Mode::Cylinder);
        p.axis = Some(DVec3::ZERO);
        assert!(Setup::new(&p, &refs, DVec3::Z).is_err());
        assert!(Mode::parse("cone").is_err());
    }
}
