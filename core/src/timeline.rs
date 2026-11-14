//! The timeline: clips played one after another for videos. A clip glides
//! through saved scenes, turns around the model (turntable), walks a path (a
//! recorded walk or points on the floor), or holds a camera while the sun
//! crosses the sky (a sun study, from the built-in solar position, no
//! online service). The engine works out every frame so the app's preview,
//! its video export and headless renders all agree.

use anyhow::{bail, Context, Result};
use glam::DVec3;
use serde::{Deserialize, Serialize};

use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};

/// Most frames one timeline makes (5 minutes at 30 fps).
pub const MAX_FRAMES: usize = 9000;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Timeline {
    #[serde(default)]
    pub clips: Vec<Clip>,
}

impl Timeline {
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Clip {
    #[serde(default)]
    pub name: Option<String>,
    /// how long it plays
    pub seconds: f64,
    #[serde(flatten)]
    pub kind: ClipKind,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Cam {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    #[serde(default = "fov")]
    pub fov: f64,
}

fn fov() -> f64 {
    40.0
}
fn one() -> f64 {
    1.0
}
fn eye_h() -> f64 {
    1.68
}
fn summer() -> String {
    "2026-06-21".into()
}
fn morning() -> String {
    "07:00".into()
}
fn evening() -> String {
    "19:00".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClipKind {
    /// Glide through saved scenes, holding `hold` seconds on each.
    Scenes {
        views: Vec<String>,
        #[serde(default = "one")]
        hold: f64,
    },
    /// Circle the model from `from` (the default 3/4 view when absent).
    Turntable {
        #[serde(default = "one")]
        turns: f64,
        #[serde(default)]
        from: Option<Cam>,
    },
    /// Walk: recorded cameras (`keys`), or floor points (`path`) walked at
    /// eye height looking ahead. Played at a steady pace.
    Walk {
        #[serde(default)]
        keys: Vec<Cam>,
        #[serde(default)]
        path: Vec<[f64; 3]>,
        #[serde(default = "eye_h")]
        eye_height: f64,
    },
    /// Hold a camera while the sun moves from `from` to `to` on `date`.
    Sun {
        #[serde(default = "summer")]
        date: String,
        #[serde(default = "morning")]
        from: String,
        #[serde(default = "evening")]
        to: String,
        #[serde(default)]
        camera: Option<Cam>,
    },
}

/// One frame: the camera, and the sun when the clip sets it.
#[derive(Clone, Debug, Serialize)]
pub struct Frame {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    pub fov: f64,
    /// [azimuth (from north, clockwise), altitude], degrees
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sun: Option<[f64; 2]>,
    /// "HH:MM" for sun studies (shown over the video)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock: Option<String>,
    /// which clip this frame is in
    pub clip: usize,
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn cam(c: &Cam, clip: usize) -> Frame {
    Frame { eye: c.eye, target: c.target, fov: c.fov, sun: None, clock: None, clip }
}

fn lerp_cam(a: &Cam, b: &Cam, t: f64, clip: usize) -> Frame {
    let l = |p: [f64; 3], q: [f64; 3]| DVec3::from(p).lerp(DVec3::from(q), t).to_array();
    Frame { eye: l(a.eye, b.eye), target: l(a.target, b.target), fov: a.fov + (b.fov - a.fov) * t, sun: None, clock: None, clip }
}

fn default_cam(m: &Model) -> Cam {
    let c = render::camera_for(m, &RenderOpts::default());
    Cam { eye: c.eye, target: c.target, fov: c.fov }
}

fn hhmm(s: &str) -> Result<f64> {
    let p: Vec<&str> = s.split(':').collect();
    let h: f64 = p.first().and_then(|x| x.trim().parse().ok()).with_context(|| format!("{s:?}: times are like 07:30"))?;
    let mi: f64 = p.get(1).and_then(|x| x.trim().parse().ok()).unwrap_or(0.0);
    Ok(h + mi / 60.0)
}

/// Every frame of the timeline at `fps`.
pub fn frames(m: &Model, tl: &Timeline, fps: f64, sun: &dyn Fn(&str, &str) -> Result<[f64; 2]>) -> Result<Vec<Frame>> {
    let fps = fps.clamp(1.0, 60.0);
    let mut out = vec![];
    for (ci, clip) in tl.clips.iter().enumerate() {
        let n = ((clip.seconds.max(0.1) * fps).round() as usize).max(1);
        if out.len() + n > MAX_FRAMES {
            bail!("the timeline is too long: the most is {} seconds at {fps} fps", MAX_FRAMES as f64 / fps);
        }
        match &clip.kind {
            ClipKind::Scenes { views, hold } => {
                let vs: Vec<Cam> = views
                    .iter()
                    .map(|name| m.views.iter().find(|v| &v.name == name).map(|v| Cam { eye: v.eye, target: v.target, fov: v.fov }).with_context(|| format!("no saved scene called {name:?}")))
                    .collect::<Result<_>>()?;
                if vs.is_empty() {
                    bail!("a scenes clip needs at least one scene");
                }
                if vs.len() == 1 {
                    out.extend((0..n).map(|_| cam(&vs[0], ci)));
                    continue;
                }
                // each scene holds, then glides to the next; the glides share what's left
                let hold_f = ((hold.max(0.0) * fps) as usize).min(n / vs.len());
                let glide_total = n.saturating_sub(hold_f * vs.len());
                let per = glide_total as f64 / (vs.len() - 1) as f64;
                let mut made = 0;
                for (i, v) in vs.iter().enumerate() {
                    for _ in 0..hold_f {
                        out.push(cam(v, ci));
                        made += 1;
                    }
                    if let Some(next) = vs.get(i + 1) {
                        let g = ((per * (i + 1) as f64).round() as usize).saturating_sub((per * i as f64).round() as usize);
                        for k in 0..g {
                            out.push(lerp_cam(v, next, smooth(k as f64 / g.max(1) as f64), ci));
                            made += 1;
                        }
                    }
                }
                for _ in made..n {
                    out.push(cam(vs.last().unwrap(), ci));
                }
            }
            ClipKind::Turntable { turns, from } => {
                let c = from.clone().unwrap_or_else(|| default_cam(m));
                let (eye, target) = (DVec3::from(c.eye), DVec3::from(c.target));
                let off = eye - target;
                for k in 0..n {
                    let a = std::f64::consts::TAU * turns * k as f64 / n as f64;
                    let (s, co) = a.sin_cos();
                    let o = DVec3::new(off.x * co - off.y * s, off.x * s + off.y * co, off.z);
                    out.push(Frame { eye: (target + o).to_array(), target: c.target, fov: c.fov, sun: None, clock: None, clip: ci });
                }
            }
            ClipKind::Walk { keys, path, eye_height } => {
                let keys: Vec<Cam> = if !keys.is_empty() {
                    keys.clone()
                } else {
                    walk_keys(path, *eye_height)?
                };
                if keys.len() < 2 {
                    bail!("a walk needs a recorded walk or two or more points");
                }
                // steady pace: by distance walked (turning on the spot counts a little)
                let mut d = vec![0.0];
                for w in keys.windows(2) {
                    let step = DVec3::from(w[0].eye).distance(DVec3::from(w[1].eye));
                    let turn = (DVec3::from(w[0].target) - DVec3::from(w[0].eye)).normalize_or_zero().angle_between((DVec3::from(w[1].target) - DVec3::from(w[1].eye)).normalize_or_zero());
                    d.push(d.last().unwrap() + step + turn.abs() * 0.4);
                }
                let total = *d.last().unwrap();
                for k in 0..n {
                    let s = if n == 1 { 0.0 } else { total * k as f64 / (n - 1) as f64 };
                    let i = d.partition_point(|x| *x <= s).clamp(1, keys.len() - 1);
                    let span = (d[i] - d[i - 1]).max(1e-9);
                    out.push(lerp_cam(&keys[i - 1], &keys[i], ((s - d[i - 1]) / span).clamp(0.0, 1.0), ci));
                }
            }
            ClipKind::Sun { date, from, to, camera } => {
                let c = camera.clone().unwrap_or_else(|| default_cam(m));
                let (a, b) = (hhmm(from)?, hhmm(to)?);
                for k in 0..n {
                    let h = a + (b - a) * k as f64 / (n - 1).max(1) as f64;
                    let clock = format!("{:02}:{:02}", h.floor() as i64, ((h.fract() * 60.0).round() as i64).min(59));
                    let mut f = cam(&c, ci);
                    f.sun = Some(sun(date, &clock)?);
                    f.clock = Some(clock);
                    out.push(f);
                }
            }
        }
    }
    Ok(out)
}

/// Cameras along floor points: eye height up, looking a few steps ahead.
fn walk_keys(path: &[[f64; 3]], eye: f64) -> Result<Vec<Cam>> {
    if path.len() < 2 {
        bail!("a walk path needs two or more points");
    }
    let pts: Vec<DVec3> = path.iter().map(|p| DVec3::from(*p) + DVec3::Z * eye).collect();
    // corners are rounded: each step looks toward a point ~2 m further along
    let mut dense = vec![];
    for w in pts.windows(2) {
        let steps = ((w[0].distance(w[1]) / 0.25).ceil() as usize).max(1);
        for k in 0..steps {
            dense.push(w[0].lerp(w[1], k as f64 / steps as f64));
        }
    }
    dense.push(*pts.last().unwrap());
    let ahead = 8;
    Ok((0..dense.len())
        .map(|i| {
            let j = (i + ahead).min(dense.len() - 1);
            let mut look = if j > i { dense[j] - dense[i] } else { dense[i] - dense[i.saturating_sub(1)] };
            look.z = 0.0;
            let look = look.normalize_or(DVec3::Y);
            Cam { eye: dense[i].to_array(), target: (dense[i] + look * 2.0).to_array(), fov: 60.0 }
        })
        .collect())
}

/// The frames as render cameras (for headless video export).
pub fn cameras(fs: &[Frame]) -> Vec<CameraSpec> {
    fs.iter().map(|f| CameraSpec { eye: f.eye, target: f.target, fov: f.fov }).collect()
}
