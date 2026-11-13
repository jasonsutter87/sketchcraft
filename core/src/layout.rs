//! Drawing sheets (a small LayOut): pages with model viewports (saved views,
//! optionally at a true scale like 1:100), a frame, labels with scale bars,
//! and a title block. Written as a vector PDF; the views are images.

use crate::model::Model;
use crate::render::{self, CameraSpec, RenderOpts};
use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Sheet sizes, landscape, in millimeters.
pub fn sheet_mm(name: &str) -> Result<(f64, f64)> {
    Ok(match name.to_ascii_lowercase().as_str() {
        "a4" => (297.0, 210.0),
        "a3" => (420.0, 297.0),
        "a2" => (594.0, 420.0),
        "letter" => (279.4, 215.9),
        "tabloid" | "11x17" => (431.8, 279.4),
        _ => bail!("sheet is a4, a3, a2, letter or tabloid"),
    })
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ViewportSpec {
    /// a saved view by name, or a standard one: iso, top, front, back, left, right
    pub view: String,
    /// "1:100" style; makes the view parallel and to scale
    pub scale: Option<String>,
    /// position and size on the sheet, mm from the top left (auto when 0)
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// caption (default: the view's name)
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PageSpec {
    pub title: String,
    pub viewports: Vec<ViewportSpec>,
}

/// "1:100" -> 100
fn scale_of(s: &str) -> Result<f64> {
    let d = s.trim().split(':').nth(1).unwrap_or(s).trim().parse::<f64>().context("scale looks like 1:100")?;
    if d <= 0.0 {
        bail!("scale must be positive");
    }
    Ok(d)
}

struct Page {
    content: String,
    images: Vec<image::RgbImage>,
}

const PT: f64 = 72.0 / 25.4;

/// Lay viewports out in a grid where they have no size of their own.
fn place(vps: &mut [ViewportSpec], sw: f64, sh: f64) {
    let n = vps.len().max(1);
    let cols = if n == 1 { 1 } else { 2 };
    let rows = n.div_ceil(cols);
    let (left, top, right, bottom) = (15.0, 15.0, 15.0, 52.0); // room for the title block
    let (cw, ch) = ((sw - left - right) / cols as f64, (sh - top - bottom) / rows as f64);
    for (i, v) in vps.iter_mut().enumerate() {
        if v.w <= 0.0 || v.h <= 0.0 {
            let (c, r) = (i % cols, i / cols);
            v.x = left + c as f64 * cw + 4.0;
            v.y = top + r as f64 * ch + 4.0;
            v.w = cw - 8.0;
            v.h = ch - 14.0; // caption below
        }
    }
}

fn esc(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).map(|c| if matches!(c, '(' | ')' | '\\') { format!("\\{c}") } else { c.to_string() }).collect()
}

pub fn write_sheets(m: &Model, path: &str, sheet: &str, project: &str, mut pages: Vec<PageSpec>) -> Result<usize> {
    let (sw, sh) = sheet_mm(sheet)?;
    // saved views in a grid, or the four standard views
    let defaults: Vec<ViewportSpec> = if m.views.is_empty() { ["iso", "top", "front", "right"].iter().map(|s| s.to_string()).collect::<Vec<_>>() } else { m.views.iter().take(4).map(|v| v.name.clone()).collect() }
        .into_iter()
        .map(|view| ViewportSpec { view, ..Default::default() })
        .collect();
    if pages.is_empty() {
        pages.push(PageSpec { title: "Views".into(), viewports: vec![] });
    }
    for p in &mut pages {
        if p.viewports.is_empty() {
            p.viewports = defaults.clone();
        }
    }
    if pages.len() > 50 {
        bail!("at most 50 pages");
    }
    let date = today();
    let total = pages.len();
    let mut out_pages = Vec::new();
    for (pi, page) in pages.iter_mut().enumerate() {
        place(&mut page.viewports, sw, sh);
        let y = |mm: f64| (sh - mm) * PT; // top-left mm -> PDF points
        let mut c = String::new();
        let mut images = Vec::new();
        // sheet border
        c += &format!("0.6 w {:.2} {:.2} {:.2} {:.2} re S\n", 10.0 * PT, 10.0 * PT, (sw - 20.0) * PT, (sh - 20.0) * PT);
        for v in &page.viewports {
            let mut o = RenderOpts { paper: true, grid: false, ..Default::default() };
            let px_per_mm = (6.0f64).min(4000.0 / v.w.max(v.h));
            o.width = (v.w * px_per_mm) as u32;
            o.height = (v.h * px_per_mm) as u32;
            let mut label = v.label.clone().unwrap_or_else(|| v.view.clone());
            if let Some(saved) = m.views.iter().find(|s| s.name == v.view) {
                o.camera = Some(CameraSpec { eye: saved.eye, target: saved.target, fov: saved.fov });
            } else {
                o.view = v.view.clone();
            }
            let mut denom = None;
            if let Some(s) = &v.scale {
                let d = scale_of(s)?;
                o.parallel = true;
                o.ortho_height = Some(v.h * d / 1000.0);
                label = format!("{label}   1:{d}");
                denom = Some(d);
            }
            images.push(render::render(m, &o));
            let n = images.len();
            c += &format!("q {:.2} 0 0 {:.2} {:.2} {:.2} cm /Im{n} Do Q\n", v.w * PT, v.h * PT, v.x * PT, y(v.y + v.h));
            c += &format!("0.3 w {:.2} {:.2} {:.2} {:.2} re S\n", v.x * PT, y(v.y + v.h), v.w * PT, v.h * PT);
            c += &format!("BT /F2 8 Tf {:.2} {:.2} Td ({}) Tj ET\n", v.x * PT, y(v.y + v.h + 5.0), esc(&label));
            if let Some(d) = denom {
                // scale bar: a round length no longer than 40 mm on paper
                let max_m = 40.0 * d / 1000.0;
                let len_m = [0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0].iter().copied().filter(|l| *l <= max_m).last().unwrap_or(max_m);
                let len_mm = len_m * 1000.0 / d;
                let (bx, by) = (v.x + v.w - len_mm, v.y + v.h + 3.5);
                c += &format!("0.5 w {:.2} {:.2} m {:.2} {:.2} l S\n", bx * PT, y(by), (bx + len_mm) * PT, y(by));
                for k in 0..=4 {
                    let tx = bx + len_mm * k as f64 / 4.0;
                    c += &format!("{:.2} {:.2} m {:.2} {:.2} l S\n", tx * PT, y(by - 1.0), tx * PT, y(by + 1.0));
                }
                c += &format!("BT /F1 6 Tf {:.2} {:.2} Td ({} m) Tj ET\n", (bx + len_mm + 1.0) * PT, y(by + 1.0), len_m);
            }
        }
        // title block, bottom right
        let (tw, th) = (110.0f64.min(sw * 0.4), 32.0);
        let (tx, ty) = (sw - 10.0 - tw, sh - 10.0 - th);
        c += &format!("0.6 w {:.2} {:.2} {:.2} {:.2} re S\n", tx * PT, y(ty + th), tw * PT, th * PT);
        c += &format!("0.3 w {:.2} {:.2} m {:.2} {:.2} l S\n", tx * PT, y(ty + 14.0), (tx + tw) * PT, y(ty + 14.0));
        c += &format!("BT /F2 12 Tf {:.2} {:.2} Td ({}) Tj ET\n", (tx + 4.0) * PT, y(ty + 9.0), esc(project));
        c += &format!("BT /F1 9 Tf {:.2} {:.2} Td ({}) Tj ET\n", (tx + 4.0) * PT, y(ty + 21.0), esc(&page.title));
        c += &format!("BT /F1 7 Tf {:.2} {:.2} Td ({}   Sheet {} of {}   Drawn in SketchCraft) Tj ET\n", (tx + 4.0) * PT, y(ty + 28.5), date, pi + 1, total);
        out_pages.push(Page { content: c, images });
    }
    write_pdf(path, sw * PT, sh * PT, &out_pages)?;
    Ok(total)
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let yr = yoe + era * 400 + i64::from(mo <= 2);
    format!("{yr:04}-{mo:02}-{d:02}")
}

fn write_pdf(path: &str, pw: f64, ph: f64, pages: &[Page]) -> Result<()> {
    use flate2::{write::ZlibEncoder, Compression};
    use std::io::Write;
    let mut objs: Vec<Vec<u8>> = vec![Vec::new(), Vec::new()]; // 1 catalog, 2 pages: filled last
    let font = |name: &str| format!("<< /Type /Font /Subtype /Type1 /BaseFont /{name} >>").into_bytes();
    objs.push(font("Helvetica")); // 3
    objs.push(font("Helvetica-Bold")); // 4
    let mut kids = Vec::new();
    for p in pages {
        let mut xobj = String::new();
        for (i, img) in p.images.iter().enumerate() {
            let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
            enc.write_all(img.as_raw())?;
            let data = enc.finish()?;
            let mut o = format!("<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n", img.width(), img.height(), data.len()).into_bytes();
            o.extend(&data);
            o.extend(b"\nendstream");
            objs.push(o);
            xobj += &format!("/Im{} {} 0 R ", i + 1, objs.len());
        }
        let mut cs = format!("<< /Length {} >>\nstream\n", p.content.len()).into_bytes();
        cs.extend(p.content.as_bytes());
        cs.extend(b"endstream");
        objs.push(cs);
        let content = objs.len();
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> /XObject << {xobj}>> >> /Contents {content} 0 R >>").into_bytes());
        kids.push(format!("{} 0 R", objs.len()));
    }
    objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    objs[1] = format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), kids.len()).into_bytes();
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in &offsets {
        out.extend(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    if let Some(dir) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(dir).ok();
    }
    std::fs::write(path, out)?;
    Ok(())
}
