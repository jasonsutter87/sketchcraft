//! Position Texture: move, rotate, scale and skew a face's picture. The change is an affine map of the picture's
//! own plane (the "picture space": u times the tile width, v times the tile height, in metres), applied to the face's
//! current coordinates and kept as the face's own corner coordinates (`Face::uv`), so it composes with a painted-through
//! placement (`Face::tex`) and with a projection alike. Design notes: docs/dev/position-texture.md.

use crate::geom;
use crate::model::{FaceUv, Id, Model, Texture};
use crate::texmap::Mapping;
use anyhow::{bail, Result};
use glam::DVec3;
use std::collections::HashMap;

/// An affine map of the picture plane: p' = [a c; b d] p + [e f].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine(pub [f64; 6]);

impl Affine {
    pub const IDENTITY: Affine = Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
    }

    pub fn det(&self) -> f64 {
        self.0[0] * self.0[3] - self.0[1] * self.0[2]
    }

    /// `self` after `first`.
    pub fn after(&self, first: &Affine) -> Affine {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = first.0;
        Affine([a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f])
    }

    /// Scale by `s`, shear by `skew` (degrees: x is the u axis leaning along v, y the v axis leaning along u), turn by
    /// `rotation` degrees, all about `pivot`, then move by `offset`.
    pub fn about(pivot: [f64; 2], offset: [f64; 2], rotation: f64, s: [f64; 2], skew: [f64; 2]) -> Affine {
        let (sin, cos) = rotation.to_radians().sin_cos();
        let rot = Affine([cos, sin, -sin, cos, 0.0, 0.0]);
        let shear = Affine([1.0, skew[1].to_radians().tan(), skew[0].to_radians().tan(), 1.0, 0.0, 0.0]);
        let scale = Affine([s[0], 0.0, 0.0, s[1], 0.0, 0.0]);
        let lin = rot.after(&shear).after(&scale);
        // p' = pivot + offset + lin (p - pivot)
        let [a, b, c, d, ..] = lin.0;
        Affine([a, b, c, d, pivot[0] + offset[0] - (a * pivot[0] + c * pivot[1]), pivot[1] + offset[1] - (b * pivot[0] + d * pivot[1])])
    }

    pub fn check(&self) -> Result<()> {
        if self.0.iter().any(|v| !v.is_finite()) {
            bail!("the transform has a value that isn't a number");
        }
        if self.det().abs() < 1e-9 {
            bail!("the transform squashes the picture flat (a scale of 0)");
        }
        Ok(())
    }
}

/// A material's lay of the picture, without the picture itself.
pub type Frames = HashMap<String, Texture>;

/// One face's picture now: per loop the corners' picture-space coordinates, the loops' points, the tile size, the material.
type Corners = (Vec<Vec<[f64; 2]>>, Vec<Vec<DVec3>>, (f64, f64), String);

impl Model {
    /// Every textured material's placement rule, without the image bytes.
    pub fn texture_frames(&self) -> Frames {
        let lite = |t: &Texture| Texture { data: String::new(), width: t.width, height: t.height, project: t.project, origin: t.origin };
        let mut out = Frames::new();
        for d in self.definitions.values() {
            for (n, m) in &d.model.materials {
                if let Some(t) = &m.texture {
                    out.insert(n.clone(), lite(t));
                }
            }
        }
        for (n, m) in &self.materials {
            if let Some(t) = &m.texture {
                out.insert(n.clone(), lite(t));
            }
        }
        out
    }

    fn picture_corners(&self, id: Id, frames: &Frames, fresh: bool) -> Result<Corners> {
        let f = self.faces.get(&id).ok_or_else(|| anyhow::anyhow!("no face {id} here (open the component to edit its faces)"))?;
        let Some(name) = f.material.as_deref() else { bail!("face {id} has no picture: paint it with a textured material first") };
        let Some(t) = frames.get(name) else { bail!("face {id}'s material \"{name}\" has no picture") };
        let loops: Vec<Vec<DVec3>> = f.loops().map(|l| self.pts(l)).collect();
        let (w, h) = (t.width.max(1e-6), t.height.max(1e-6));
        let own = f.uv.as_ref().filter(|u| u.fits(f) && !fresh);
        let uv: Vec<Vec<[f64; 2]>> = match own {
            Some(u) => std::iter::once(&u.outer).chain(u.holes.iter()).map(|r| r.iter().map(|p| [p[0] as f64, p[1] as f64]).collect()).collect(),
            None => {
                let place = f.tex.as_ref().filter(|p| !fresh && f.material.as_deref() == Some(p.material.as_str()));
                let map = Mapping::of_place(t, place, geom::newell(&loops[0]));
                loops.iter().map(|l| l.iter().map(|p| map.uv(*p)).collect()).collect()
            }
        };
        let pic = uv.into_iter().map(|l| l.into_iter().map(|p| [p[0] * w, p[1] * h]).collect()).collect();
        Ok((pic, loops, (w, h), name.to_string()))
    }

    /// What Position needs to draw its pins: per loop the corners' points and picture-space coordinates, the tile size.
    pub fn texture_frame(&self, id: Id, frames: &Frames, fresh: bool) -> Result<serde_json::Value> {
        let (pic, loops, (w, h), material) = self.picture_corners(id, frames, fresh)?;
        let f = &self.faces[&id];
        let n = geom::newell(&loops[0]);
        Ok(serde_json::json!({
            "material": material, "tile": [w, h], "normal": [n.x, n.y, n.z],
            "own": !fresh && f.uv.as_ref().is_some_and(|u| u.fits(f)),
            "points": loops.iter().map(|l| l.iter().map(|p| [p.x, p.y, p.z]).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "picture": pic,
        }))
    }

    /// Change the pictures of faces by the map `m` (picture space); `m` gets each face's middle (its default pivot).
    /// Returns how many faces changed. Nothing changes if it is refused.
    pub fn position_texture(&mut self, ids: &[Id], m: &dyn Fn([f64; 2]) -> Affine, frames: &Frames) -> Result<usize> {
        self.check_unlocked(ids)?;
        let mut faces = self.faces_of(ids)?;
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() {
            bail!("select a face to position its texture");
        }
        let mut done = Vec::with_capacity(faces.len());
        for id in faces {
            let (pic, _, (w, h), _) = self.picture_corners(id, frames, false)?;
            let n = pic.iter().map(Vec::len).sum::<usize>().max(1) as f64;
            let mid = pic.iter().flatten().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
            let a = m(mid);
            a.check()?;
            let back = |r: &Vec<[f64; 2]>| r.iter().map(|p| a.apply(*p)).map(|p| [(p[0] / w) as f32, (p[1] / h) as f32]).collect::<Vec<_>>();
            let mut rings = pic.iter().map(back);
            let outer = rings.next().unwrap_or_default();
            done.push((id, FaceUv { outer, holes: rings.collect() }));
        }
        let n = done.len();
        for (id, uv) in done {
            self.faces.get_mut(&id).unwrap().uv = Some(uv);
        }
        Ok(n)
    }

    /// Put faces' pictures back to the material's own placement (drops both the projection and the painted-through place).
    pub fn reset_texture_position(&mut self, ids: &[Id]) -> Result<usize> {
        self.check_unlocked(ids)?;
        let mut n = 0;
        for id in self.faces_of(ids)? {
            let f = self.faces.get_mut(&id).unwrap();
            n += (f.uv.take().is_some() | f.tex.take().is_some()) as usize;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn move_adds_the_offset() {
        let a = Affine::about([0.3, 0.4], [1.0, -2.0], 0.0, [1.0, 1.0], [0.0, 0.0]);
        assert!(close(a.apply([5.0, 5.0]), [6.0, 3.0]));
    }

    #[test]
    fn rotate_turns_about_the_pivot_and_leaves_it_put() {
        let a = Affine::about([1.0, 1.0], [0.0, 0.0], 90.0, [1.0, 1.0], [0.0, 0.0]);
        assert!(close(a.apply([1.0, 1.0]), [1.0, 1.0]));
        assert!(close(a.apply([2.0, 1.0]), [1.0, 2.0]));
    }

    #[test]
    fn scale_grows_away_from_the_pivot() {
        let a = Affine::about([1.0, 0.0], [0.0, 0.0], 0.0, [2.0, 3.0], [0.0, 0.0]);
        assert!(close(a.apply([2.0, 1.0]), [3.0, 3.0]));
    }

    #[test]
    fn skew_leans_one_axis_along_the_other() {
        let a = Affine::about([0.0, 0.0], [0.0, 0.0], 0.0, [1.0, 1.0], [45.0, 0.0]);
        assert!(close(a.apply([0.0, 2.0]), [2.0, 2.0]));
        assert!(close(a.apply([3.0, 0.0]), [3.0, 0.0]));
    }

    #[test]
    fn compose_and_flip() {
        let flip = Affine::about([0.5, 0.0], [0.0, 0.0], 0.0, [-1.0, 1.0], [0.0, 0.0]);
        assert!(close(flip.apply([0.0, 1.0]), [1.0, 1.0]));
        let twice = flip.after(&flip);
        assert!(close(twice.apply([0.2, 0.7]), [0.2, 0.7]));
        let t = Affine::about([0.0; 2], [1.0, 0.0], 0.0, [1.0; 2], [0.0; 2]);
        let r = Affine::about([0.0; 2], [0.0; 2], 90.0, [1.0; 2], [0.0; 2]);
        // rotate then move
        assert!(close(t.after(&r).apply([1.0, 0.0]), [1.0, 1.0]));
    }

    #[test]
    fn a_flat_or_broken_map_is_refused() {
        assert!(Affine::about([0.0; 2], [0.0; 2], 0.0, [0.0, 1.0], [0.0; 2]).check().is_err());
        assert!(Affine([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]).check().is_err());
        assert!(Affine::IDENTITY.check().is_ok());
    }
}
