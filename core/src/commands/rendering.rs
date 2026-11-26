//! Pictures of the model and where it stands: renders, thumbnails, the sun and the environment.
//! Arms of `Engine::run`; each works on the model being edited (inside `edit`, the open component's definition).

use super::*;

impl Engine {
    pub(super) fn run_rendering(&mut self, cmd: Command) -> Result<Value> {
        let m: &mut Model = active_model!(self);
        Ok(match cmd {
            Command::Photoreal { out, samples, bounces, mut sun_azimuth, mut sun_altitude, sun_strength, denoise, exposure, sky_strength, background, ground, ground_color, warmth, date, time, utc_offset, progressive, opts } => {
                // laid out, the photoreal picture shows what the view shows (an owned copy: `self.pt` changes below)
                let laid = self.laid_out_view().map(|v| v.model);
                let model: &Model = laid.as_ref().unwrap_or(&self.model);
                let mut sun_note = None;
                if date.is_some() || time.is_some() {
                    let s = sun_at(model, date.as_deref(), time.as_deref(), utc_offset)?;
                    if s.altitude < 1.0 {
                        bail!("the sun is down then ({:.0}° below the horizon); pick a daytime hour", -s.altitude);
                    }
                    sun_azimuth = Some(s.azimuth);
                    sun_altitude = s.altitude;
                    sun_note = Some(serde_json::to_value(&s)?);
                }
                let camera = crate::pt::camera(model, &opts);
                // flat figures turn to face this camera
                let turned = model.facing(glam::DVec3::from(camera.eye));
                let model = turned.as_ref().unwrap_or(model);
                let po = crate::pt::PtOpts {
                    width: opts.width,
                    height: opts.height,
                    samples: samples.clamp(1, 4096),
                    bounces: bounces.clamp(1, 16),
                    camera: camera.clone(),
                    sun: crate::pt::sun_dir(
                        sun_azimuth.unwrap_or_else(|| {
                            // three-quarter light: over the camera's right shoulder, so the face you see is lit and shadows fall beside it
                            let (e, t) = (glam::DVec3::from(camera.eye), glam::DVec3::from(camera.target));
                            let back = e - t;
                            let az = back.x.atan2(back.y).to_degrees();
                            az + 45.0
                        }),
                        sun_altitude,
                    ),
                    sun_strength,
                    denoise,
                    exposure: exposure.clamp(0.01, 64.0),
                    sky_strength: sky_strength.clamp(0.0, 10.0),
                    background: background.as_deref().map(hex_rgb).transpose()?,
                    ground,
                    ground_color: ground_color.as_deref().map(hex_rgb).transpose()?,
                    warmth: warmth.clamp(-1.0, 1.0),
                }
                .limited();
                let t0 = std::time::Instant::now();
                let mut spp = po.samples;
                let img = if progressive {
                    // same picture as last time? keep adding to it
                    // switching to or from the laid-out parts changes the picture, though not the model
                    let key = format!("{}{}|{:?}|{:?}|{}|{}|{}|{}|{}", self.revision, self.laid_out.as_ref().map(|r| format!("L{:?}{:?}", r.ids, r.gap)).unwrap_or_default(), po.camera.eye, po.camera.target, po.camera.fov, po.width, po.height, po.sun, po.bounces) + &format!("|{}|{}|{}|{:?}|{}|{:?}|{}", po.sun_strength, po.exposure, po.sky_strength, po.background, po.ground, po.ground_color, po.warmth);
                    let (p, img) = crate::pt::progressive(self.pt.take(), key, model, &po);
                    spp = p.spp;
                    self.pt = Some(p);
                    img
                } else {
                    crate::pt::render(model, &po)
                };
                let secs = t0.elapsed().as_secs_f64();
                let data = if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    crate::safety::save_image(&img, path)?;
                    None
                } else {
                    use base64::Engine as _;
                    let mut buf = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut buf, image::ImageFormat::Png)?;
                    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(buf.into_inner())))
                };
                json!({"out": out, "data": data, "seconds": secs, "spp": spp, "sun": sun_note, "camera": {"eye": camera.eye, "target": camera.target, "fov": camera.fov}})
            }
            Command::Render { out, opts } => {
                // laid out, a render shows what the view shows
                let shown = self.shown_model();
                let whole: &Model = &shown;
                let cam = render::camera_for(whole, &opts);
                if let Some(path) = &out {
                    if let Some(dir) = std::path::Path::new(path).parent() {
                        std::fs::create_dir_all(dir).ok();
                    }
                    // flat figures turn to face this camera
                    let turned = whole.facing(glam::DVec3::from(cam.eye));
                    let fixed = render::RenderOpts { camera: Some(cam.clone()), ..opts.clone() };
                    crate::safety::save_image(&render::render(turned.as_ref().unwrap_or(whole), &fixed), path)?;
                }
                json!({"out": out, "camera": {"eye": cam.eye, "target": cam.target, "fov": cam.fov}})
            }
            Command::Environment { .. } => bail!("environment runs through Engine::exec"),
            Command::Sun { date, time, utc_offset } => serde_json::to_value(sun_at(&self.model, date.as_deref(), time.as_deref(), utc_offset)?)?,
            Command::AddLocation { lat, lon, size, cells, imagery } => {
                let p = crate::geo::add_location(m, lat, lon, size, cells, imagery, &crate::geo::fetch)?;
                json!({"group": p.group, "elevation": p.elevation, "zoom": p.zoom})
            }
            Command::Thumbnail { component, asset, path, size } => {
                use base64::Engine as _;
                let px = size.clamp(32, 512);
                let png = if let Some(c) = component {
                    crate::library::thumbnail_of(m, c, px)?
                } else if let Some(a) = asset {
                    let mut scratch = Model::default();
                    let id = crate::assets::definition(&mut scratch, &a)?;
                    crate::library::thumbnail_of(&scratch, id, px)?
                } else if let Some(p) = path {
                    crate::library::thumbnail_of_file(&p, px)?
                } else {
                    bail!("thumbnail needs a component, asset or path");
                };
                json!({"data": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))})
            }
            Command::SaveImage { path, data } => {
                use base64::Engine as _;
                let b64 = data.strip_prefix("data:image/png;base64,").ok_or_else(|| anyhow::anyhow!("expected a PNG data URL"))?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
                // only a real picture is written: decode it (this also bounds its size)
                let img = crate::safety::decode_image(&bytes, Some(image::ImageFormat::Png)).map_err(|e| anyhow::anyhow!("that isn't a PNG image the program can save: {e}"))?;
                let _ = img;
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    std::fs::create_dir_all(dir).ok();
                }
                crate::safety::write_atomic(std::path::Path::new(&path), &bytes)?;
                json!({"path": path})
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }
}
