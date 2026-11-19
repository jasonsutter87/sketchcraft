//! Axis-aligned (rectilinear) polygons and exact 2D booleans on them, for Room mode.
//!
//! Every edge of these polygons runs along x or y, so the booleans use coordinate compression: cut the plane by every
//! distinct x and y of both shapes, and each grid cell is wholly in or out of each shape. The result is the boundary
//! between the kept and dropped cells. No tolerance games: coordinates within 1e-9 of each other are one coordinate.
//!
//! A polygon is a list of corners, counter-clockwise for an outer outline (clockwise for a hole in a result).

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

pub type Pt = [f64; 2];
pub type Poly = Vec<Pt>;

const SAME: f64 = 1e-9;

/// What to do with two shapes. Union and Subtract ship in Room mode; Intersect ("Inner") is here for the next step, and
/// Outer (a xor b) and Split (the pieces of a cut) are also one `keep` rule away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Union,
    Subtract,
    Intersect,
}

impl Op {
    fn keep(self, a: bool, b: bool) -> bool {
        match self {
            Op::Union => a || b,
            Op::Subtract => a && !b,
            Op::Intersect => a && b,
        }
    }
}

/// One straight edge of a polygon: along x (`horizontal`) or y, at `v` on the other axis, from `lo` to `hi`.
/// `fwd`: the polygon runs along it toward larger coordinates (counter-clockwise: south and east sides go forward).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    pub horizontal: bool,
    pub v: f64,
    pub lo: f64,
    pub hi: f64,
    pub fwd: bool,
}

impl Edge {
    /// Which side of the shape this edge is (the way its outside faces), for a counter-clockwise outline.
    pub fn side(&self) -> &'static str {
        match (self.horizontal, self.fwd) {
            (true, true) => "south",
            (true, false) => "north",
            (false, true) => "east",
            (false, false) => "west",
        }
    }
}

pub fn rect_poly(r: [f64; 4]) -> Poly {
    vec![[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// Signed area (positive for counter-clockwise).
pub fn signed_area(p: &[Pt]) -> f64 {
    let n = p.len();
    (0..n).map(|i| p[i][0] * p[(i + 1) % n][1] - p[(i + 1) % n][0] * p[i][1]).sum::<f64>() / 2.0
}

/// Area of an outline.
pub fn area(p: &[Pt]) -> f64 {
    signed_area(p).abs()
}

pub fn bbox(p: &[Pt]) -> [f64; 4] {
    p.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, q| [b[0].min(q[0]), b[1].min(q[1]), b[2].max(q[0]), b[3].max(q[1])])
}

/// Is every edge along x or y?
pub fn is_rectilinear(p: &[Pt]) -> bool {
    let n = p.len();
    n >= 4
        && (0..n).all(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            (a[0] - b[0]).abs() < 1e-7 || (a[1] - b[1]).abs() < 1e-7
        })
}

/// Counter-clockwise, with repeated corners and corners in the middle of a straight run removed.
pub fn simplify(p: &[Pt]) -> Poly {
    let mut q: Poly = Vec::new();
    for &pt in p {
        if q.last().is_none_or(|l| (l[0] - pt[0]).abs() > 1e-7 || (l[1] - pt[1]).abs() > 1e-7) {
            q.push(pt);
        }
    }
    while q.len() > 1 && (q[0][0] - q[q.len() - 1][0]).abs() < 1e-7 && (q[0][1] - q[q.len() - 1][1]).abs() < 1e-7 {
        q.pop();
    }
    loop {
        let n = q.len();
        if n < 3 {
            break;
        }
        let Some(i) = (0..n).find(|&i| {
            let (a, b, c) = (q[(i + n - 1) % n], q[i], q[(i + 1) % n]);
            ((b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])).abs() < 1e-9
        }) else {
            break;
        };
        q.remove(i);
    }
    if signed_area(&q) < 0.0 {
        q.reverse();
    }
    q
}

/// The edges of a rectilinear polygon, in order (edge `i` runs from corner `i` to corner `i + 1`).
pub fn edges(p: &[Pt]) -> Vec<Edge> {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            if (a[1] - b[1]).abs() < 1e-7 {
                Edge { horizontal: true, v: a[1], lo: a[0].min(b[0]), hi: a[0].max(b[0]), fwd: b[0] > a[0] }
            } else {
                Edge { horizontal: false, v: a[0], lo: a[1].min(b[1]), hi: a[1].max(b[1]), fwd: b[1] > a[1] }
            }
        })
        .collect()
}

/// Is `q` inside the polygon (a point exactly on an edge may go either way)?
pub fn contains(p: &[Pt], q: Pt) -> bool {
    let n = p.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (p[i], p[(i + 1) % n]);
        if (a[1] > q[1]) != (b[1] > q[1]) && q[0] < a[0] + (q[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]) {
            inside = !inside;
        }
    }
    inside
}

/// Move every edge of a counter-clockwise rectilinear polygon `d` outward (negative: inward).
pub fn offset(p: &[Pt], d: f64) -> Poly {
    let n = p.len();
    let normal = |a: Pt, b: Pt| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = dx.hypot(dy);
        [dy / l, -dx / l]
    };
    (0..n)
        .map(|i| {
            let n1 = normal(p[(i + n - 1) % n], p[i]);
            let n2 = normal(p[i], p[(i + 1) % n]);
            [p[i][0] + (n1[0] + n2[0]) * d, p[i][1] + (n1[1] + n2[1]) * d]
        })
        .collect()
}

/// Sorted distinct coordinates (values within `SAME` are one).
fn distinct(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::new();
    for x in v {
        if out.last().is_none_or(|l| x - l > SAME) {
            out.push(x);
        }
    }
    out
}

struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
}

impl Grid {
    fn of(a: &[Pt], b: &[Pt]) -> Grid {
        Grid { xs: distinct(a.iter().chain(b).map(|p| p[0]).collect()), ys: distinct(a.iter().chain(b).map(|p| p[1]).collect()) }
    }
    fn nx(&self) -> usize {
        self.xs.len().saturating_sub(1)
    }
    fn ny(&self) -> usize {
        self.ys.len().saturating_sub(1)
    }
    fn centre(&self, i: usize, j: usize) -> Pt {
        [(self.xs[i] + self.xs[i + 1]) / 2.0, (self.ys[j] + self.ys[j + 1]) / 2.0]
    }
    fn cell_area(&self, i: usize, j: usize) -> f64 {
        (self.xs[i + 1] - self.xs[i]) * (self.ys[j + 1] - self.ys[j])
    }
}

/// The area the two shapes have in common.
pub fn overlap_area(a: &[Pt], b: &[Pt]) -> f64 {
    let g = Grid::of(a, b);
    let mut s = 0.0;
    for i in 0..g.nx() {
        for j in 0..g.ny() {
            let c = g.centre(i, j);
            if contains(a, c) && contains(b, c) {
                s += g.cell_area(i, j);
            }
        }
    }
    s
}

/// `a` combined with `b` by `op`, as outlines: counter-clockwise outer outlines and clockwise holes (corners simplified).
/// A result whose shapes touch only at a corner is refused: it has no clean outline.
pub fn boolean(a: &[Pt], b: &[Pt], op: Op) -> Result<Vec<Poly>> {
    let g = Grid::of(a, b);
    let (nx, ny) = (g.nx(), g.ny());
    let mut keep = vec![false; nx * ny];
    for i in 0..nx {
        for j in 0..ny {
            let c = g.centre(i, j);
            keep[i * ny + j] = op.keep(contains(a, c), contains(b, c));
        }
    }
    let at = |i: isize, j: isize| i >= 0 && j >= 0 && (i as usize) < nx && (j as usize) < ny && keep[i as usize * ny + j as usize];
    // directed boundary edges with the kept cell on the left, between grid corners (i, j)
    let mut next: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
    for i in 0..nx {
        for j in 0..ny {
            if !keep[i * ny + j] {
                continue;
            }
            let (ii, jj) = (i as isize, j as isize);
            if !at(ii, jj - 1) {
                next.entry((i, j)).or_default().push((i + 1, j));
            }
            if !at(ii + 1, jj) {
                next.entry((i + 1, j)).or_default().push((i + 1, j + 1));
            }
            if !at(ii, jj + 1) {
                next.entry((i + 1, j + 1)).or_default().push((i, j + 1));
            }
            if !at(ii - 1, jj) {
                next.entry((i, j + 1)).or_default().push((i, j));
            }
        }
    }
    if next.values().any(|v| v.len() > 1) {
        bail!("the result would touch itself at a corner: nudge the box so the shapes overlap or leave a clear gap");
    }
    let mut loops: Vec<Poly> = Vec::new();
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut starts: Vec<(usize, usize)> = next.keys().copied().collect();
    starts.sort();
    for s in starts {
        if seen.contains(&s) {
            continue;
        }
        let mut pts = Vec::new();
        let mut cur = s;
        loop {
            seen.insert(cur);
            pts.push([g.xs[cur.0], g.ys[cur.1]]);
            cur = next[&cur][0];
            if cur == s {
                break;
            }
        }
        // `simplify` turns clockwise loops round, so put a hole back the way it was
        let hole = signed_area(&pts) < 0.0;
        let mut q = simplify(&pts);
        if hole {
            q.reverse();
        }
        loops.push(q);
    }
    Ok(loops)
}

/// As `boolean`, for a result that must be one plain outline (no holes, one piece): the counter-clockwise outline, or why not.
pub fn boolean_single(a: &[Pt], b: &[Pt], op: Op) -> Result<Poly> {
    let loops = boolean(a, b, op)?;
    let outers: Vec<&Poly> = loops.iter().filter(|l| signed_area(l) > 0.0).collect();
    if loops.len() > outers.len() {
        bail!("that would leave a hole in the room: drag the box to an edge of the room instead of into the middle");
    }
    match outers.len() {
        0 => bail!("that would leave nothing of the room"),
        1 => Ok(outers[0].clone()),
        _ => bail!("that would split the room in two: a room has to stay in one piece"),
    }
}

/// A point to hang a label on: inside the outline, near its middle. The centroid when that is inside (a rectangle's is
/// its centre); otherwise the centre of the largest cell of the outline's grid.
pub fn label_point(p: &[Pt]) -> Pt {
    let a = signed_area(p);
    let n = p.len();
    if a.abs() > 1e-12 {
        let (mut cx, mut cy) = (0.0, 0.0);
        for i in 0..n {
            let (u, v) = (p[i], p[(i + 1) % n]);
            let w = u[0] * v[1] - v[0] * u[1];
            cx += (u[0] + v[0]) * w;
            cy += (u[1] + v[1]) * w;
        }
        let c = [cx / (6.0 * a), cy / (6.0 * a)];
        if contains(p, c) {
            return c;
        }
    }
    let g = Grid::of(p, &[]);
    let mut best: Option<(f64, Pt)> = None;
    for i in 0..g.nx() {
        for j in 0..g.ny() {
            let c = g.centre(i, j);
            if contains(p, c) && best.is_none_or(|(s, _)| g.cell_area(i, j) > s) {
                best = Some((g.cell_area(i, j), c));
            }
        }
    }
    best.map_or_else(|| p.first().copied().unwrap_or([0.0, 0.0]), |(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Poly {
        rect_poly([x0, y0, x1, y1])
    }

    #[test]
    fn union_of_overlapping_squares_is_an_l() {
        let r = boolean_single(&sq(0.0, 0.0, 4.0, 4.0), &sq(2.0, 2.0, 6.0, 6.0), Op::Union).unwrap();
        assert_eq!(r.len(), 8);
        assert!((area(&r) - 28.0).abs() < 1e-9);
        assert!(signed_area(&r) > 0.0);
    }

    #[test]
    fn union_with_a_box_off_one_side_is_an_l() {
        let r = boolean_single(&sq(0.0, 0.0, 4.0, 4.0), &sq(2.0, 3.0, 4.0, 6.0), Op::Union).unwrap();
        assert_eq!(r.len(), 6);
        assert!((area(&r) - (16.0 + 2.0 * 2.0)).abs() < 1e-9);
    }

    #[test]
    fn subtract_a_corner_leaves_an_l_and_an_edge_bite_a_notch() {
        let l = boolean_single(&sq(0.0, 0.0, 4.0, 4.0), &sq(2.0, 2.0, 6.0, 6.0), Op::Subtract).unwrap();
        assert_eq!(l.len(), 6);
        assert!((area(&l) - 12.0).abs() < 1e-9);
        let notch = boolean_single(&sq(0.0, 0.0, 4.0, 4.0), &sq(3.0, 1.0, 6.0, 3.0), Op::Subtract).unwrap();
        assert_eq!(notch.len(), 8);
        assert!((area(&notch) - 14.0).abs() < 1e-9);
    }

    #[test]
    fn two_overlaps_make_a_t() {
        let bar = boolean_single(&sq(0.0, 0.0, 6.0, 2.0), &sq(2.0, 1.0, 4.0, 5.0), Op::Union).unwrap();
        assert_eq!(bar.len(), 8);
        assert!((area(&bar) - (12.0 + 2.0 * 3.0)).abs() < 1e-9);
        assert!(contains(&bar, [3.0, 4.0]) && !contains(&bar, [1.0, 4.0]));
    }

    #[test]
    fn a_box_covering_everything() {
        let r = boolean_single(&sq(1.0, 1.0, 3.0, 3.0), &sq(0.0, 0.0, 4.0, 4.0), Op::Union).unwrap();
        assert_eq!(r, sq(0.0, 0.0, 4.0, 4.0));
        assert!(boolean_single(&sq(1.0, 1.0, 3.0, 3.0), &sq(0.0, 0.0, 4.0, 4.0), Op::Subtract).is_err());
        let inner = boolean_single(&sq(1.0, 1.0, 3.0, 3.0), &sq(0.0, 0.0, 4.0, 4.0), Op::Intersect).unwrap();
        assert!((area(&inner) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn disjoint_shapes_and_touching_edges() {
        let far = boolean(&sq(0.0, 0.0, 2.0, 2.0), &sq(5.0, 5.0, 7.0, 7.0), Op::Union).unwrap();
        assert_eq!(far.len(), 2);
        let e = boolean_single(&sq(0.0, 0.0, 2.0, 2.0), &sq(5.0, 5.0, 7.0, 7.0), Op::Union).unwrap_err().to_string();
        assert!(e.contains("split"), "{e}");
        // sharing a whole edge: one rectangle
        let joined = boolean_single(&sq(0.0, 0.0, 2.0, 2.0), &sq(2.0, 0.0, 5.0, 2.0), Op::Union).unwrap();
        assert_eq!(joined, sq(0.0, 0.0, 5.0, 2.0));
        // sharing part of an edge: a step
        let step = boolean_single(&sq(0.0, 0.0, 2.0, 2.0), &sq(2.0, 1.0, 5.0, 4.0), Op::Union).unwrap();
        assert_eq!(step.len(), 8);
        // touching edges subtract nothing
        let same = boolean_single(&sq(0.0, 0.0, 2.0, 2.0), &sq(2.0, 0.0, 5.0, 2.0), Op::Subtract).unwrap();
        assert_eq!(same, sq(0.0, 0.0, 2.0, 2.0));
        // only at a corner: refused
        assert!(boolean(&sq(0.0, 0.0, 2.0, 2.0), &sq(2.0, 2.0, 4.0, 4.0), Op::Union).is_err());
    }

    #[test]
    fn through_the_middle_is_refused() {
        let e = boolean_single(&sq(0.0, 0.0, 6.0, 6.0), &sq(2.0, 2.0, 4.0, 4.0), Op::Subtract).unwrap_err().to_string();
        assert!(e.contains("hole"), "{e}");
        let e = boolean_single(&sq(0.0, 0.0, 6.0, 4.0), &sq(2.0, -1.0, 3.0, 5.0), Op::Subtract).unwrap_err().to_string();
        assert!(e.contains("two"), "{e}");
    }

    #[test]
    fn offset_simplify_and_labels() {
        let l = boolean_single(&sq(0.0, 0.0, 4.0, 4.0), &sq(2.0, 2.0, 6.0, 6.0), Op::Union).unwrap();
        let inner = offset(&l, -0.1);
        assert_eq!(inner.len(), 8);
        // inward by d: area - perimeter * d + d^2 * (convex corners - reflex corners) = 28 - 2.4 + 0.01 * 4
        assert!((area(&inner) - (28.0 - 2.4 + 0.04)).abs() < 1e-9, "{}", area(&inner));
        assert_eq!(simplify(&[[0.0, 0.0], [2.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]), sq(0.0, 0.0, 4.0, 4.0));
        assert_eq!(label_point(&sq(0.0, 0.0, 4.0, 2.0)), [2.0, 1.0]);
        // a U: the centroid is in the gap, the label is not
        let u = boolean_single(&sq(0.0, 0.0, 9.0, 6.0), &sq(3.0, 2.0, 6.0, 7.0), Op::Subtract).unwrap();
        let c = label_point(&u);
        assert!(contains(&u, c), "{c:?}");
    }
}
