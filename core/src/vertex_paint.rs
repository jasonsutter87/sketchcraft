//! Vertex painting: brush strokes that tint the corners of faces (`Face::vcol`, one RGBA per corner).
//!
//! The blend and falloff maths are plain functions, tested on their own. [`Model::vertex_paint`] finds the corners a
//! stroke reaches and blends them; [`Model::vertex_paint_clear`] takes the colours off. Design notes:
//! docs/dev/vertex-paint.md.

use crate::model::{FaceColors, Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;

pub const WHITE: [f32; 4] = [1.0; 4];
/// The most points one stroke carries (the app thins a long drag to 1,500).
pub const MAX_STROKE_POINTS: usize = 10_000;
/// The most corner-against-point tests one stroke may make (about a second of work); a stroke past it is refused before
/// anything changes. Only the points that reach a face's box are tested at its corners.
pub const MAX_STROKE_WORK: u64 = 1 << 29;

/// Count `n` more units of a stroke's work; refused once the stroke is past `MAX_STROKE_WORK`.
fn spend(work: &mut u64, n: u64) -> Result<()> {
    *work = work.saturating_add(n);
    if *work > MAX_STROKE_WORK {
        bail!("that stroke is too much work at once (over {} million corner tests): use a smaller brush, fewer points or fewer faces", MAX_STROKE_WORK / 1_000_000);
    }
    Ok(())
}

/// How the brush weakens from its centre (distance 0) to its rim (distance `radius`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Falloff {
    /// smoothstep: full strength in the middle, soft edge (default)
    Smooth,
    /// straight line from full to nothing
    Linear,
    /// full strength everywhere inside the radius
    Hard,
}

impl Falloff {
    pub fn parse(s: &str) -> Result<Falloff> {
        Ok(match s {
            "smooth" => Falloff::Smooth,
            "linear" => Falloff::Linear,
            "hard" | "none" => Falloff::Hard,
            other => bail!("unknown falloff \"{other}\": use smooth, linear or hard"),
        })
    }

    /// Weight 0..1 at `d` from the centre of a brush of radius `r`.
    pub fn weight(self, d: f64, r: f64) -> f64 {
        if d > r {
            return 0.0;
        }
        let t = if r <= 0.0 { 0.0 } else { (d / r).clamp(0.0, 1.0) };
        match self {
            Falloff::Hard => 1.0,
            Falloff::Linear => 1.0 - t,
            Falloff::Smooth => 1.0 - t * t * (3.0 - 2.0 * t),
        }
    }
}

/// `old` moved toward `target` by `w` (0 keeps it, 1 replaces it), per channel.
pub fn blend(old: [f32; 4], target: [f32; 4], w: f64) -> [f32; 4] {
    let w = w.clamp(0.0, 1.0) as f32;
    let mut o = [0.0; 4];
    for k in 0..4 {
        o[k] = old[k] + (target[k] - old[k]) * w;
    }
    o
}

/// The brush weight at point `p`: the strongest of the stroke's points (a stroke never piles up where its dabs overlap).
pub fn stroke_weight(p: DVec3, points: &[DVec3], radius: f64, falloff: Falloff) -> f64 {
    points.iter().map(|q| falloff.weight(p.distance(*q), radius)).fold(0.0, f64::max)
}

/// A colour read from a command: [r, g, b] or [r, g, b, a], each 0..1.
pub fn parse_color(c: &[f32]) -> Result<[f32; 4]> {
    if c.len() != 3 && c.len() != 4 {
        bail!("color is [r, g, b] or [r, g, b, a], each 0..1");
    }
    if c.iter().any(|x| !x.is_finite()) {
        bail!("color must be numbers");
    }
    let a = c.get(3).copied().unwrap_or(1.0);
    Ok([c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), a.clamp(0.0, 1.0)])
}

/// What one stroke does.
#[derive(Clone, Debug)]
pub struct Stroke {
    pub color: [f32; 4],
    /// the stroke's points (world, metres); empty: fill the whole selection
    pub points: Vec<DVec3>,
    pub radius: f64,
    pub strength: f64,
    pub falloff: Falloff,
}

/// The colours at the corners of a world face, looked up by the corner's exact point (the triangulator hands back the
/// loop's own points, as for texture coordinates).
pub struct CornerColors(Option<std::collections::HashMap<[u64; 3], [f32; 4]>>);

impl CornerColors {
    pub fn of(f: &crate::components::WFace) -> CornerColors {
        let key = |p: &DVec3| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        CornerColors(f.vcol.as_ref().map(|(o, h)| {
            let mut m = std::collections::HashMap::new();
            for (p, c) in f.outer.iter().zip(o) {
                m.insert(key(p), *c);
            }
            for (ring, cs) in f.holes.iter().zip(h) {
                for (p, c) in ring.iter().zip(cs) {
                    m.insert(key(p), *c);
                }
            }
            m
        }))
    }

    /// Does the face carry vertex colours?
    pub fn any(&self) -> bool {
        self.0.is_some()
    }

    /// RGBA at a corner point (white where the face has none).
    pub fn at(&self, p: DVec3) -> [f32; 4] {
        self.0.as_ref().and_then(|m| m.get(&[p.x.to_bits(), p.y.to_bits(), p.z.to_bits()])).copied().unwrap_or(WHITE)
    }

    /// The tint the viewport multiplies the face with: the colour weakened toward white by its alpha.
    pub fn tint(&self, p: DVec3) -> [f32; 3] {
        let c = self.at(p);
        [1.0 + (c[0] - 1.0) * c[3], 1.0 + (c[1] - 1.0) * c[3], 1.0 + (c[2] - 1.0) * c[3]]
    }
}

impl Model {
    /// Paint a stroke on faces (or every face of groups). Returns (faces changed, corners changed). Nothing changes
    /// if it is refused.
    pub fn vertex_paint(&mut self, ids: &[Id], s: &Stroke) -> Result<(usize, usize)> {
        self.vertex_paint_within(ids, s, &mut 0)
    }

    /// `vertex_paint`, counting its work into `work` (one command painting several placed components shares one
    /// budget); refused once that passes `MAX_STROKE_WORK`.
    pub fn vertex_paint_within(&mut self, ids: &[Id], s: &Stroke, work: &mut u64) -> Result<(usize, usize)> {
        self.check_unlocked(ids)?;
        if !s.radius.is_finite() || !s.strength.is_finite() {
            bail!("radius and strength must be numbers");
        }
        if !s.points.is_empty() && s.radius <= 0.0 {
            bail!("radius must be above 0");
        }
        if s.points.len() > MAX_STROKE_POINTS {
            bail!("a stroke has at most {MAX_STROKE_POINTS} points ({} given): send a long stroke as several", s.points.len());
        }
        let mut faces = self.faces_of(ids)?;
        faces.sort_unstable();
        faces.dedup();
        if faces.is_empty() {
            bail!("select some faces (or a group) to paint");
        }
        let mut points = s.points.clone();
        points.dedup();
        // the box the whole stroke reaches: faces outside it are passed over without looking at the points
        let pad = DVec3::splat(s.radius * (1.0 + 1e-9) + 1e-12);
        let reach = points.iter().fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p - pad), hi.max(*p + pad)));
        // a face's corner points and the stroke's points that reach its box (None: none do), with the work that costs
        let look = |f: &crate::model::Face| -> (Vec<Vec<DVec3>>, Option<Vec<DVec3>>, u64) {
            let loops: Vec<Vec<DVec3>> = f.loops().map(|l| self.pts(l)).collect();
            let corners = loops.iter().map(Vec::len).sum::<usize>() as u64;
            if points.is_empty() {
                return (loops, Some(vec![]), corners);
            }
            let (lo, hi) = loops.iter().flatten().fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            if lo.cmpgt(reach.1).any() || hi.cmplt(reach.0).any() {
                return (loops, None, 1);
            }
            let near: Vec<DVec3> = points.iter().copied().filter(|q| (lo - *q).max(*q - hi).max(DVec3::ZERO).length() <= s.radius * (1.0 + 1e-9) + 1e-12).collect();
            let units = 1 + points.len() as u64 + near.len() as u64 * corners;
            (loops, (!near.is_empty()).then_some(near), units)
        };
        // the work is counted first, so a stroke over the budget is refused before any corner is weighed
        for id in &faces {
            spend(work, look(&self.faces[id]).2)?;
        }
        let (mut nf, mut nc) = (0, 0);
        let mut done: Vec<(Id, Option<FaceColors>)> = vec![];
        for id in faces {
            let f = &self.faces[&id];
            let (loops, near, _) = look(f);
            let Some(near) = near else { continue };
            let mut cur = match &f.vcol {
                Some(c) if c.fits(f) => c.clone(),
                _ => FaceColors { outer: vec![WHITE; f.outer.len()], holes: f.holes.iter().map(|h| vec![WHITE; h.len()]).collect() },
            };
            let mut changed = 0;
            let mut apply = |ring: &mut Vec<[f32; 4]>, pts: &[DVec3]| {
                for (c, p) in ring.iter_mut().zip(pts) {
                    let w = if points.is_empty() { 1.0 } else { stroke_weight(*p, &near, s.radius, s.falloff) };
                    let w = w * s.strength;
                    if w > 0.0 {
                        let n = blend(*c, s.color, w);
                        if n != *c {
                            *c = n;
                            changed += 1;
                        }
                    }
                }
            };
            apply(&mut cur.outer, &loops[0]);
            for (h, pts) in cur.holes.iter_mut().zip(&loops[1..]) {
                apply(h, pts);
            }
            if changed == 0 {
                continue;
            }
            nf += 1;
            nc += changed;
            let all_white = cur.outer.iter().chain(cur.holes.iter().flatten()).all(|c| *c == WHITE);
            done.push((id, if all_white { None } else { Some(cur) }));
        }
        for (id, c) in done {
            self.faces.get_mut(&id).unwrap().vcol = c;
        }
        Ok((nf, nc))
    }

    /// Take vertex colours off faces. Returns how many had them.
    pub fn vertex_paint_clear(&mut self, ids: &[Id]) -> Result<usize> {
        self.check_unlocked(ids)?;
        let mut n = 0;
        for id in self.faces_of(ids)? {
            n += self.faces.get_mut(&id).unwrap().vcol.take().is_some() as usize;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_shapes() {
        for f in [Falloff::Smooth, Falloff::Linear, Falloff::Hard] {
            assert_eq!(f.weight(0.0, 1.0), 1.0);
            assert_eq!(f.weight(1.5, 1.0), 0.0);
        }
        assert!((Falloff::Linear.weight(0.25, 1.0) - 0.75).abs() < 1e-12);
        assert!((Falloff::Smooth.weight(0.5, 1.0) - 0.5).abs() < 1e-12);
        assert_eq!(Falloff::Hard.weight(1.0, 1.0), 1.0);
        assert_eq!(Falloff::Linear.weight(1.0, 1.0), 0.0);
        // smooth is steadier than linear in the middle and gentler at the rim
        assert!(Falloff::Smooth.weight(0.2, 1.0) > Falloff::Linear.weight(0.2, 1.0));
        assert!(Falloff::Smooth.weight(0.8, 1.0) < Falloff::Linear.weight(0.8, 1.0));
        assert!(Falloff::parse("sharp").is_err());
    }

    #[test]
    fn blend_moves_toward_the_target() {
        let red = [1.0, 0.0, 0.0, 1.0];
        assert_eq!(blend(WHITE, red, 0.0), WHITE);
        assert_eq!(blend(WHITE, red, 1.0), red);
        assert_eq!(blend(WHITE, red, 0.5), [1.0, 0.5, 0.5, 1.0]);
        // repeated strokes converge on the colour
        let mut c = WHITE;
        for _ in 0..40 {
            c = blend(c, red, 0.2);
        }
        assert!(c[1] < 0.001);
        // out of range weights are clamped
        assert_eq!(blend(WHITE, red, 7.0), red);
        assert_eq!(blend(WHITE, red, -1.0), WHITE);
    }

    #[test]
    fn stroke_takes_the_strongest_dab() {
        let pts = [DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0)];
        let p = DVec3::new(0.5, 0.0, 0.0);
        let one = Falloff::Linear.weight(0.5, 1.0);
        assert!((stroke_weight(p, &pts, 1.0, Falloff::Linear) - one).abs() < 1e-12);
        assert_eq!(stroke_weight(DVec3::new(9.0, 0.0, 0.0), &pts, 1.0, Falloff::Smooth), 0.0);
        assert_eq!(stroke_weight(p, &[], 1.0, Falloff::Smooth), 0.0);
    }

    #[test]
    fn colors_parse() {
        assert_eq!(parse_color(&[0.5, 2.0, -1.0]).unwrap(), [0.5, 1.0, 0.0, 1.0]);
        assert_eq!(parse_color(&[0.1, 0.2, 0.3, 0.4]).unwrap()[3], 0.4);
        assert!(parse_color(&[1.0, 1.0]).is_err());
        assert!(parse_color(&[f32::NAN, 0.0, 0.0]).is_err());
    }
}
