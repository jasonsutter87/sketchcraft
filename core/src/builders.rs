//! Building blocks: walls along a path, stairs from a rise, roofs on a face,
//! columns. Each makes ordinary geometry in its own group, so it can be
//! edited with every other tool afterwards.

use crate::geom;
use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::{DMat4, DVec2, DVec3};

impl Model {
    /// Add a polygon facing `outward` (flipped if it came out the other way).
    fn facing_poly(&mut self, pts: &[DVec3], outward: DVec3) -> Result<Id> {
        let f = self.add_polygon(pts)?;
        if self.normal(f).dot(outward) < 0.0 {
            self.faces.get_mut(&f).unwrap().reverse();
        }
        Ok(f)
    }

    fn in_new_group<T>(&mut self, name: &str, f: impl FnOnce(&mut Model) -> Result<T>) -> Result<(Id, T)> {
        let prev = self.active_group;
        let gid = self.begin_group(name.to_string());
        let r = f(self);
        self.active_group = prev;
        match r {
            Ok(v) => Ok((gid, v)),
            Err(e) => {
                self.delete(&[gid]).ok();
                Err(e)
            }
        }
    }

    /// Walls `thickness` thick and `height` tall, centered on a path of points
    /// (on the ground plane of the first point). `closed` makes a room.
    pub fn wall(&mut self, points: &[DVec3], thickness: f64, height: f64, closed: bool) -> Result<Id> {
        let mut pts: Vec<DVec3> = points.to_vec();
        pts.dedup_by(|a, b| a.distance(*b) < 1e-6);
        if closed && pts.len() > 2 && pts[0].distance(*pts.last().unwrap()) < 1e-6 {
            pts.pop();
        }
        if pts.len() < 2 || (closed && pts.len() < 3) {
            bail!("a wall needs at least two points (three for a closed room)");
        }
        if thickness <= 0.0 || height <= 0.0 {
            bail!("thickness and height must be positive");
        }
        let z = pts[0].z;
        let p2: Vec<DVec2> = pts.iter().map(|p| p.truncate()).collect();
        let n = p2.len();
        let half = thickness / 2.0;
        let left = |a: DVec2, b: DVec2| {
            let d = (b - a).normalize();
            DVec2::new(-d.y, d.x)
        };
        // miter offset of the path by `off` (positive = left)
        let offset = |off: f64| -> Vec<DVec3> {
            (0..n)
                .map(|i| {
                    let (prev, next) = if closed {
                        (Some(p2[(i + n - 1) % n]), Some(p2[(i + 1) % n]))
                    } else {
                        (if i > 0 { Some(p2[i - 1]) } else { None }, if i + 1 < n { Some(p2[i + 1]) } else { None })
                    };
                    let m = match (prev, next) {
                        (Some(a), Some(b)) => {
                            let (n1, n2) = (left(a, p2[i]), left(p2[i], b));
                            (n1 + n2) / (1.0 + n1.dot(n2)).max(0.15)
                        }
                        (None, Some(b)) => left(p2[i], b),
                        (Some(a), None) => left(a, p2[i]),
                        _ => DVec2::ZERO,
                    };
                    (p2[i] + m * off).extend(z)
                })
                .collect()
        };
        let (l, r) = (offset(half), offset(-half));
        let (gid, _) = self.in_new_group("Wall", |m| {
            let base = if closed {
                // ring: outer loop with the inner loop as a hole
                let ccw = geom::signed_area(&p2) > 0.0;
                let (outer, inner) = if ccw { (r, l) } else { (l, r) };
                let f = m.add_polygon(&outer)?;
                let hole = m.add_polygon(&inner)?;
                m.delete(&[hole])?;
                f
            } else {
                let mut poly = l.clone();
                poly.extend(r.iter().rev().copied());
                m.add_polygon(&poly)?
            };
            m.pushpull(base, height)?;
            Ok(())
        })?;
        Ok(gid)
    }

    /// A straight stair: `rise` total height, risers no taller than 19 cm,
    /// treads `tread` deep, `width` wide. Climbs along `direction` (degrees
    /// from +X) starting at `at`.
    pub fn stairs(&mut self, at: DVec3, direction: f64, width: f64, rise: f64, tread: f64) -> Result<(Id, usize)> {
        if width <= 0.0 || rise <= 0.0 || tread <= 0.0 {
            bail!("width, rise and tread must be positive");
        }
        let n = (rise / 0.19).ceil().max(1.0) as usize;
        let r = rise / n as f64;
        let t = DMat4::from_translation(at) * DMat4::from_rotation_z(direction.to_radians());
        // profile in the local XZ plane (y = 0): risers and treads, solid underneath
        let mut prof = vec![DVec3::ZERO];
        for k in 1..=n {
            let x = (k - 1) as f64 * tread;
            prof.push(DVec3::new(x, 0.0, k as f64 * r));
            if k < n {
                prof.push(DVec3::new(k as f64 * tread, 0.0, k as f64 * r));
            }
        }
        prof.push(DVec3::new((n - 1) as f64 * tread, 0.0, 0.0));
        if n == 1 {
            prof = vec![DVec3::ZERO, DVec3::new(0.0, 0.0, r), DVec3::new(tread, 0.0, r), DVec3::new(tread, 0.0, 0.0)];
        }
        let world: Vec<DVec3> = prof.iter().map(|p| t.transform_point3(*p)).collect();
        let side = t.transform_vector3(DVec3::Y);
        let (gid, _) = self.in_new_group("Stairs", |m| {
            let f = m.add_polygon(&world)?;
            let d = if m.normal(f).dot(side) >= 0.0 { width } else { -width };
            m.pushpull(f, d)?;
            Ok(())
        })?;
        Ok((gid, n))
    }

    /// A roof over a face's outline: "gable" (ridge along the long side),
    /// "hip" or "shed". `pitch` in degrees, `overhang` past the walls,
    /// `thickness` of the roof slab.
    pub fn roof(&mut self, face: Id, kind: &str, pitch: f64, overhang: f64, thickness: f64) -> Result<Id> {
        let pts = self.pts(&self.face(face)?.outer.clone());
        let n = geom::newell(&pts);
        if n.z.abs() < 0.7 {
            bail!("pick a flat top face for the roof");
        }
        // the outline's long direction, from its longest edge
        let k = pts.len();
        let (mut best, mut len) = (DVec3::X, 0.0);
        for i in 0..k {
            let e = pts[(i + 1) % k] - pts[i];
            let e2 = DVec3::new(e.x, e.y, 0.0);
            if e2.length() > len {
                len = e2.length();
                best = e2.normalize();
            }
        }
        let ax = best;
        let ay = DVec3::Z.cross(ax);
        let z0 = pts.iter().map(|p| p.z).fold(f64::MIN, f64::max);
        let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in &pts {
            x0 = x0.min(p.dot(ax));
            x1 = x1.max(p.dot(ax));
            y0 = y0.min(p.dot(ay));
            y1 = y1.max(p.dot(ay));
        }
        let at = |x: f64, y: f64, z: f64| ax * x + ay * y + DVec3::Z * z;
        let tan = pitch.to_radians().tan();
        if !(0.0..80.0).contains(&pitch) {
            bail!("pitch must be between 0 and 80 degrees");
        }
        let (w, l) = (y1 - y0, x1 - x0);
        let (ox0, ox1, oy0, oy1) = (x0 - overhang, x1 + overhang, y0 - overhang, y1 + overhang);
        let name = format!("{} roof", match kind { "hip" => "Hip", "shed" => "Shed", _ => "Gable" });
        let (gid, _) = self.in_new_group(&name, |m| {
            match kind {
                "shed" => {
                    // one slope rising across the short side, closed at the gable ends
                    let rise = w * tan;
                    let drop = overhang * tan;
                    let a = at(ox0, oy0, z0 - drop);
                    let b = at(ox1, oy0, z0 - drop);
                    let c = at(ox1, oy1, z0 + rise + drop);
                    let d = at(ox0, oy1, z0 + rise + drop);
                    let top = m.add_polygon(&[a, b, c, d])?;
                    if thickness > 0.0 {
                        m.pushpull(top, thickness)?;
                    }
                    // gable-end infill down to the wall top
                    m.facing_poly(&[at(x0, y0, z0), at(x0, y1, z0), at(x0, y1, z0 + rise)], -ax)?;
                    m.facing_poly(&[at(x1, y0, z0), at(x1, y1, z0 + rise), at(x1, y1, z0)], ax)?;
                }
                "hip" => {
                    let rise = w / 2.0 * tan;
                    let drop = overhang * tan;
                    let ze = z0 - drop;
                    let half = (l - w) / 2.0;
                    let ym = (y0 + y1) / 2.0;
                    let xm = (x0 + x1) / 2.0;
                    let r0 = at(xm - half.max(0.0), ym, z0 + rise);
                    let r1 = at(xm + half.max(0.0), ym, z0 + rise);
                    let c = [at(ox0, oy0, ze), at(ox1, oy0, ze), at(ox1, oy1, ze), at(ox0, oy1, ze)];
                    if half > 1e-6 {
                        m.add_polygon(&[c[0], c[1], r1, r0])?;
                        m.add_polygon(&[c[2], c[3], r0, r1])?;
                        m.add_polygon(&[c[1], c[2], r1])?;
                        m.add_polygon(&[c[3], c[0], r0])?;
                    } else {
                        for i in 0..4 {
                            m.add_polygon(&[c[i], c[(i + 1) % 4], r0])?;
                        }
                    }
                    let soffit = m.add_polygon(&[c[0], c[3], c[2], c[1]])?;
                    let _ = soffit;
                }
                _ => {
                    // gable: a slab with a V profile at one end, pulled the full length
                    let rise = w / 2.0 * tan;
                    let drop = overhang * tan;
                    let ym = (y0 + y1) / 2.0;
                    let t = thickness.max(0.01) / pitch.to_radians().cos().max(0.1);
                    let prof = [
                        at(ox0, oy0, z0 - drop),
                        at(ox0, ym, z0 + rise),
                        at(ox0, oy1, z0 - drop),
                        at(ox0, oy1, z0 - drop + t),
                        at(ox0, ym, z0 + rise + t),
                        at(ox0, oy0, z0 - drop + t),
                    ];
                    let f = m.add_polygon(&prof)?;
                    let d = if m.normal(f).dot(ax) >= 0.0 { ox1 - ox0 } else { -(ox1 - ox0) };
                    m.pushpull(f, d)?;
                    // close the gable ends over the walls
                    m.facing_poly(&[at(x0, y0, z0), at(x0, y1, z0), at(x0, ym, z0 + rise)], -ax)?;
                    m.facing_poly(&[at(x1, y0, z0), at(x1, ym, z0 + rise), at(x1, y1, z0)], ax)?;
                }
            }
            Ok(())
        })?;
        Ok(gid)
    }

    /// A square or round column `size` across and `height` tall, standing at `at`.
    pub fn column(&mut self, at: DVec3, size: f64, height: f64, round: bool) -> Result<Id> {
        if size <= 0.0 || height <= 0.0 {
            bail!("size and height must be positive");
        }
        let (gid, _) = self.in_new_group("Column", |m| {
            let f = if round {
                m.circle(at, DVec3::Z, size / 2.0, 24)?
            } else {
                let h = size / 2.0;
                m.add_polygon(&[at + DVec3::new(-h, -h, 0.0), at + DVec3::new(h, -h, 0.0), at + DVec3::new(h, h, 0.0), at + DVec3::new(-h, h, 0.0)])?
            };
            m.pushpull(f, height)?;
            Ok(())
        })?;
        Ok(gid)
    }
}
