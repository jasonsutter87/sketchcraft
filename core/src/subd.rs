//! Subdivision surfaces (Catmull-Clark): model a coarse cage of quads (and
//! any n-gons), and get a smooth surface, the way Blender's Subdivision
//! modifier works. Edges marked as creases stay sharp; open borders keep
//! their outline. Each cage face's material carries to the faces it becomes,
//! so glass painted on the cage is smooth glass on the result.

use crate::model::{Id, Model};
use anyhow::{bail, Result};
use glam::DVec3;
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct Cage {
    pub points: Vec<DVec3>,
    pub faces: Vec<Vec<usize>>,
    /// per face (None: unpainted)
    pub materials: Vec<Option<String>>,
    /// sharp edges (vertex index pairs, either order)
    pub creases: HashSet<(usize, usize)>,
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// One Catmull-Clark step: every face becomes quads (one per corner).
pub fn step(c: &Cage) -> Cage {
    let n = c.points.len();
    // face points
    let fp: Vec<DVec3> = c.faces.iter().map(|f| f.iter().map(|&i| c.points[i]).sum::<DVec3>() / f.len() as f64).collect();
    // edges and the faces on them
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (fi, f) in c.faces.iter().enumerate() {
        for k in 0..f.len() {
            edge_faces.entry(key(f[k], f[(k + 1) % f.len()])).or_default().push(fi);
        }
    }
    let sharp = |e: &(usize, usize), fs: &Vec<usize>| fs.len() != 2 || c.creases.contains(e);
    // edge points
    let mut ep: HashMap<(usize, usize), usize> = HashMap::new();
    let mut points: Vec<DVec3> = Vec::with_capacity(n + edge_faces.len() + c.faces.len());
    // new positions for the old points come first (same indexes)
    let mut sum_f: Vec<DVec3> = vec![DVec3::ZERO; n];
    let mut cnt_f: Vec<usize> = vec![0; n];
    for (fi, f) in c.faces.iter().enumerate() {
        for &i in f {
            sum_f[i] += fp[fi];
            cnt_f[i] += 1;
        }
    }
    let mut mid_sum: Vec<DVec3> = vec![DVec3::ZERO; n];
    let mut valence: Vec<usize> = vec![0; n];
    let mut sharp_nb: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (e, fs) in &edge_faces {
        let mid = (c.points[e.0] + c.points[e.1]) / 2.0;
        for v in [e.0, e.1] {
            mid_sum[v] += mid;
            valence[v] += 1;
        }
        if sharp(e, fs) {
            sharp_nb[e.0].push(e.1);
            sharp_nb[e.1].push(e.0);
        }
    }
    for i in 0..n {
        let p = c.points[i];
        let v = match sharp_nb[i].len() {
            // a corner (3+ sharp edges) stays put
            k if k > 2 => p,
            // on a crease or a border: only the crease neighbors pull
            2 => (p * 6.0 + c.points[sharp_nb[i][0]] + c.points[sharp_nb[i][1]]) / 8.0,
            _ if valence[i] == 0 || cnt_f[i] == 0 => p,
            _ => {
                let nv = valence[i] as f64;
                let f = sum_f[i] / cnt_f[i] as f64;
                let r = mid_sum[i] / nv;
                (f + r * 2.0 + p * (nv - 3.0)) / nv
            }
        };
        points.push(v);
    }
    for (e, fs) in &edge_faces {
        let mid = (c.points[e.0] + c.points[e.1]) / 2.0;
        let p = if sharp(e, fs) { mid } else { (c.points[e.0] + c.points[e.1] + fp[fs[0]] + fp[fs[1]]) / 4.0 };
        ep.insert(*e, points.len());
        points.push(p);
    }
    let base = points.len();
    points.extend(fp.iter().copied());
    // new faces: one quad per corner of every old face
    let mut faces = Vec::with_capacity(c.faces.len() * 4);
    let mut materials = Vec::with_capacity(c.faces.len() * 4);
    for (fi, f) in c.faces.iter().enumerate() {
        let m = f.len();
        for k in 0..m {
            let (prev, cur, next) = (f[(k + m - 1) % m], f[k], f[(k + 1) % m]);
            faces.push(vec![cur, ep[&key(cur, next)], base + fi, ep[&key(prev, cur)]]);
            materials.push(c.materials[fi].clone());
        }
    }
    // a crease splits in two and stays a crease
    let mut creases = HashSet::new();
    for e in &c.creases {
        if let Some(&mid) = ep.get(&key(e.0, e.1)) {
            creases.insert(key(e.0, mid));
            creases.insert(key(mid, e.1));
        }
    }
    Cage { points, faces, materials, creases }
}

/// Subdivide `levels` times and build the result as a group of soft-edged
/// triangles. Returns the group.
pub fn build(m: &mut Model, mut cage: Cage, levels: usize, name: &str) -> Result<Id> {
    if cage.faces.is_empty() {
        bail!("the cage has no faces");
    }
    for f in &cage.faces {
        if f.len() < 3 || f.iter().any(|&i| i >= cage.points.len()) {
            bail!("each cage face needs 3+ corners, all in the point list");
        }
    }
    let levels = levels.clamp(0, 5);
    for _ in 0..levels {
        cage = step(&cage);
        if cage.faces.len() > 400_000 {
            bail!("that's over 400,000 faces: use fewer levels or a lighter cage");
        }
    }
    let g = m.begin_group(name.to_string());
    let mut mesh = crate::mesh::Mesh::new(m, Some(g));
    for (f, mat) in cage.faces.iter().zip(&cage.materials) {
        let p: Vec<DVec3> = f.iter().map(|&i| cage.points[i]).collect();
        // two triangles per quad (always flat), split along the shorter diagonal
        if p.len() == 4 {
            let (a, b) = if p[0].distance(p[2]) <= p[1].distance(p[3]) { ([p[0], p[1], p[2]], [p[0], p[2], p[3]]) } else { ([p[0], p[1], p[3]], [p[1], p[2], p[3]]) };
            mesh.face(&a, &[], mat.clone());
            mesh.face(&b, &[], mat.clone());
        } else {
            let c = p.iter().copied().sum::<DVec3>() / p.len() as f64;
            for k in 0..p.len() {
                mesh.face(&[p[k], p[(k + 1) % p.len()], c], &[], mat.clone());
            }
        }
    }
    mesh.finish();
    m.active_group = m.groups.get(&g).and_then(|x| x.parent);
    Ok(g)
}

/// A group's faces as a cage (to subdivide what's already modeled).
pub fn cage_of(m: &Model, group: Id, keep_sharp: bool) -> Result<Cage> {
    let faces = m.faces_of(&[group])?;
    let mut index: HashMap<Id, usize> = HashMap::new();
    let mut points = Vec::new();
    let mut out = Vec::new();
    let mut materials = Vec::new();
    for f in faces {
        let face = &m.faces[&f];
        if !face.holes.is_empty() {
            bail!("faces with holes can't be subdivided: split them first");
        }
        let ring: Vec<usize> = face
            .outer
            .iter()
            .map(|v| {
                *index.entry(*v).or_insert_with(|| {
                    points.push(m.p(*v));
                    points.len() - 1
                })
            })
            .collect();
        out.push(ring);
        materials.push(face.material.clone());
    }
    // hard (not soft) edges between two faces at a sharp angle become creases
    let mut creases = HashSet::new();
    for e in m.edges.values().filter(|e| keep_sharp && !e.soft && e.group.is_some_and(|g| m.subtree(group).contains(&g))) {
        let (Some(&a), Some(&b)) = (index.get(&e.a), index.get(&e.b)) else { continue };
        let users = m.faces_using_edge(e.a, e.b);
        if users.len() == 2 {
            let (n1, n2) = (m.normal(users[0]), m.normal(users[1]));
            if n1.dot(n2) < 0.5 {
                creases.insert(key(a, b));
            }
        }
    }
    Ok(Cage { points, faces: out, materials, creases })
}
