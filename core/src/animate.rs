//! Scene animation: the camera holds on each saved view, then glides to the
//! next (eased), like SketchUp's animation export. With fewer than two
//! views it circles the model instead (a turntable). Frames render on every
//! core; ffmpeg (when installed) turns them into an MP4.

use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};
use anyhow::{bail, Context, Result};
use glam::DVec3;
use std::path::{Path, PathBuf};

/// Most frames one export will render (a minute at 20 fps).
const MAX_FRAMES: usize = 1200;

pub struct Anim {
    pub fps: f64,
    /// seconds gliding from one view to the next
    pub transition: f64,
    /// seconds holding on each view
    pub delay: f64,
    /// which saved views, in order (all of them when empty)
    pub views: Vec<String>,
    /// seconds for one turn when there are no views to visit
    pub turntable: f64,
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// The camera for every frame.
pub fn cameras(m: &Model, a: &Anim, base: &RenderOpts) -> Result<Vec<CameraSpec>> {
    let fps = a.fps.clamp(1.0, 60.0);
    let views: Vec<_> = if a.views.is_empty() {
        m.views.clone()
    } else {
        a.views.iter().map(|n| m.views.iter().find(|v| &v.name == n).cloned().with_context(|| format!("no saved view called {n:?}"))).collect::<Result<_>>()?
    };
    let mut cams = Vec::new();
    if views.len() < 2 {
        // turntable around the model, from where the default camera looks
        let c = render::camera_for(m, base);
        let (eye, target) = (DVec3::from(c.eye), DVec3::from(c.target));
        let off = eye - target;
        let n = (a.turntable.max(1.0) * fps) as usize;
        for k in 0..n {
            let ang = std::f64::consts::TAU * k as f64 / n as f64;
            let (s, co) = ang.sin_cos();
            let o = DVec3::new(off.x * co - off.y * s, off.x * s + off.y * co, off.z);
            cams.push(CameraSpec { eye: (target + o).to_array(), target: target.to_array(), fov: c.fov });
        }
    } else {
        let hold = (a.delay.max(0.0) * fps) as usize;
        let glide = (a.transition.max(0.0) * fps).max(1.0) as usize;
        for (i, v) in views.iter().enumerate() {
            let here = CameraSpec { eye: v.eye, target: v.target, fov: v.fov };
            for _ in 0..hold.max(1) {
                cams.push(here.clone());
            }
            let Some(next) = views.get(i + 1) else { break };
            for k in 1..glide {
                let t = smooth(k as f64 / glide as f64);
                let l = |a: [f64; 3], b: [f64; 3]| DVec3::from(a).lerp(DVec3::from(b), t).to_array();
                cams.push(CameraSpec { eye: l(v.eye, next.eye), target: l(v.target, next.target), fov: v.fov + (next.fov - v.fov) * t });
            }
        }
    }
    if cams.len() > MAX_FRAMES {
        bail!("that's {} frames; the most is {MAX_FRAMES} (lower the fps or shorten the timing)", cams.len());
    }
    Ok(cams)
}

/// Render the animation to `path`: a folder of PNG frames, or an .mp4/.mov
/// (needs ffmpeg). Returns the number of frames.
pub fn animate(m: &Model, path: &str, a: &Anim, base: &RenderOpts) -> Result<usize> {
    let cams = cameras(m, a, base)?;
    let video = matches!(Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("mp4" | "mov"));
    let dir: PathBuf = if video { std::env::temp_dir().join(format!("sketchcraft-frames-{}", std::process::id())) } else { PathBuf::from(path) };
    std::fs::create_dir_all(&dir)?;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::Mutex::new(None::<String>);
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= cams.len() {
                    break;
                }
                let mut o = base.clone();
                o.camera = Some(cams[i].clone());
                if let Err(e) = render::render(m, &o).save(dir.join(format!("frame_{i:05}.png"))) {
                    *failed.lock().unwrap() = Some(e.to_string());
                    break;
                }
            });
        }
    });
    if let Some(e) = failed.into_inner().unwrap() {
        bail!("rendering a frame failed: {e}");
    }
    if video {
        let out = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-framerate"])
            .arg(format!("{}", a.fps.clamp(1.0, 60.0)))
            .arg("-i")
            .arg(dir.join("frame_%05d.png"))
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"])
            .arg(path)
            .output();
        std::fs::remove_dir_all(&dir).ok();
        match out {
            Ok(o) if o.status.success() => {}
            Ok(o) => bail!("ffmpeg couldn't make the video: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")),
            Err(_) => bail!("making a video needs ffmpeg installed; or give a folder path to get the frames as PNGs"),
        }
    }
    Ok(cams.len())
}
