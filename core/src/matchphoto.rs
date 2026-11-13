//! Match Photo: find the camera that took a photo from lines drawn along
//! its edges. Two lines that are parallel along the red axis in the world
//! meet at a vanishing point in the photo; so do two along the green axis.
//! From those two points (and the photo's center as the lens axis):
//!
//! - focal length: the directions to two perpendicular vanishing points are
//!   perpendicular, so f^2 = -(v_red . v_green) (centered pixel coordinates)
//! - orientation: the red and green axes point at (or away from) their
//!   vanishing points, the way the first line of each pair is drawn; blue
//!   is their cross product, kept pointing up in the photo
//! - position: the world origin sits on the ray through the origin marker,
//!   `distance` meters out (that sets the scale)

use anyhow::{bail, Result};
use glam::{DMat3, DVec2, DVec3};

pub struct PhotoCamera {
    pub eye: DVec3,
    pub target: DVec3,
    /// vertical field of view, degrees
    pub fov: f64,
}

/// Where two image lines (x1, y1, x2, y2) meet.
fn meet(a: [f64; 4], b: [f64; 4]) -> Option<DVec2> {
    let (p, r) = (DVec2::new(a[0], a[1]), DVec2::new(a[2] - a[0], a[3] - a[1]));
    let (q, s) = (DVec2::new(b[0], b[1]), DVec2::new(b[2] - b[0], b[3] - b[1]));
    let den = r.perp_dot(s);
    if den.abs() < 1e-9 * r.length() * s.length() {
        return None;
    }
    Some(p + r * ((q - p).perp_dot(s) / den))
}

/// Solve the camera. Lines and the origin are in photo pixels (x right,
/// y down); `width` x `height` is the photo's size.
pub fn solve(width: f64, height: f64, red: [[f64; 4]; 2], green: [[f64; 4]; 2], origin: [f64; 2], distance: f64) -> Result<PhotoCamera> {
    let c = DVec2::new(width / 2.0, height / 2.0);
    // centered, y up
    let cen = |p: DVec2| DVec2::new(p.x - c.x, c.y - p.y);
    let (Some(vr), Some(vg)) = (meet(red[0], red[1]), meet(green[0], green[1])) else {
        bail!("each pair of lines must point toward a common point (they're parallel in the photo)");
    };
    let (vr, vg) = (cen(vr), cen(vg));
    let f2 = -vr.dot(vg);
    if f2 <= 0.0 {
        bail!("those lines don't make a real perspective: the red and green vanishing points must be on opposite sides of the photo's center");
    }
    let f = f2.sqrt();
    // camera space: x right, y up, looking down -z
    // A vanishing point can't tell +axis from -axis. The first line of each
    // pair is drawn toward +axis: if going that way approaches the vanishing
    // point, the axis points into the photo, else out of it.
    let o = cen(DVec2::new(origin[0], origin[1]));
    let along = |l: [f64; 4]| DVec2::new(l[2] - l[0], l[1] - l[3]);
    let ahead = |vp: DVec2, l: [f64; 4]| if (vp - o).dot(along(l)) >= 0.0 { 1.0 } else { -1.0 };
    let x = DVec3::new(vr.x, vr.y, -f).normalize() * ahead(vr, red[0]);
    let mut y = DVec3::new(vg.x, vg.y, -f).normalize() * ahead(vg, green[0]);
    let mut z = x.cross(y).normalize();
    if z.y < 0.0 {
        // blue must point up in the photo
        y = -y;
        z = -z;
    }
    let y = z.cross(x).normalize();
    // world axes in camera space are the columns: camera <- world
    let m = DMat3::from_cols(x, y, z);
    let world_from_cam = m.transpose();
    let ray = DVec3::new(o.x, o.y, -f).normalize();
    let eye = -(world_from_cam * (ray * distance.max(0.01)));
    let forward = world_from_cam * DVec3::new(0.0, 0.0, -1.0);
    let fov = 2.0 * (height / 2.0 / f).atan().to_degrees();
    Ok(PhotoCamera { eye, target: eye + forward * distance.max(0.01), fov })
}
