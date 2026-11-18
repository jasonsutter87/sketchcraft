//! Sheets, animation and video.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_media(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Animate { path, fps, transition, delay, views, turntable, timeline, opts } => {
                let fps = fps.clamp(1.0, 60.0);
                if timeline {
                    let model = &self.model;
                    let fs = crate::timeline::frames(model, &model.timeline, fps, &|d, t| timeline_sun(model, d, t))?;
                    if fs.is_empty() {
                        bail!("the timeline is empty: add a clip first");
                    }
                    return Ok(json!({"path": path, "frames": crate::animate::animate_cams(model, &path, &crate::timeline::cameras(&fs), fps, &opts)?}));
                }
                let a = crate::animate::Anim { fps, transition, delay, views, turntable };
                json!({"path": path, "frames": crate::animate::animate(&self.model, &path, &a, &opts)?})
            }
            Command::Blueprint { path, sheet, project, notes } => {
                let project = project.unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Project".into()));
                let building = crate::environment::without(&self.model);
                json!({"path": path, "pages": crate::layout::blueprint(&building, &path, &sheet, &project, &notes)?, "levels": crate::layout::floor_levels(&building)})
            }
            Command::Levels => json!({"levels": crate::layout::floor_levels(&crate::environment::without(&self.model))}),
            Command::Timeline { clips } => {
                m.timeline = crate::timeline::Timeline { clips };
                // check it now, so a bad clip is caught when it's made
                let model = &*m;
                let n = crate::timeline::frames(model, &model.timeline, 10.0, &|d, t| timeline_sun(model, d, t))?.len();
                json!({"clips": m.timeline.clips.len(), "seconds": m.timeline.clips.iter().map(|c| c.seconds).sum::<f64>(), "frames_at_10fps": n})
            }
            Command::TimelineFrames { fps } => {
                let fps = fps.clamp(1.0, 60.0);
                let model = &self.model;
                let fs = crate::timeline::frames(model, &model.timeline, fps, &|d, t| timeline_sun(model, d, t))?;
                json!({"fps": fps, "frames": fs})
            }
            Command::VideoFrame { session, index, data } => {
                use base64::Engine as _;
                if index >= crate::timeline::MAX_FRAMES || data.len() > 40 << 20 {
                    bail!("that frame is out of range");
                }
                let dir = video_dir(&session)?;
                std::fs::create_dir_all(&dir)?;
                let (head, b64) = data.split_once(',').unwrap_or(("", &data));
                let ext = if head.contains("jpeg") { "jpg" } else { "png" };
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim())?;
                std::fs::write(dir.join(format!("frame_{index:05}.{ext}")), bytes)?;
                json!({"index": index})
            }
            Command::VideoFinish { session, path, fps } => {
                let dir = video_dir(&session)?;
                let ext = if dir.join("frame_00000.jpg").exists() { "jpg" } else { "png" };
                let mut c = std::process::Command::new("ffmpeg");
                c.args(["-y", "-loglevel", "error", "-framerate"])
                    .arg(format!("{}", fps.clamp(1.0, 60.0)))
                    .arg("-i")
                    .arg(dir.join(format!("frame_%05d.{ext}")))
                    .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"])
                    .arg(crate::safety::path_arg(&path));
                let out = crate::helper::run(c, crate::helper::VIDEO);
                let frames = std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0);
                std::fs::remove_dir_all(&dir).ok();
                match out {
                    Ok(o) if o.status.success() => json!({"path": path, "frames": frames}),
                    Ok(o) => bail!("ffmpeg couldn't make the video: {}", String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("")),
                    Err(e) if e.kind() == std::io::ErrorKind::TimedOut => bail!("ffmpeg {}", e),
                    Err(_) => bail!("making a video needs ffmpeg installed (brew install ffmpeg)"),
                }
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
