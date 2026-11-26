//! DWG, AutoCAD's own (closed, binary) format, through a converter the user
//! installs: LibreDWG (`brew install libredwg`, GPL, run as a separate
//! program) or the free ODA File Converter. SketchCraft reads and writes DXF
//! itself; the converter only translates between DXF and DWG.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

const HELP: &str = "Opening and saving DWG needs a converter: install LibreDWG (brew install libredwg) or the free ODA File Converter, then try again. DXF works without one.";

fn find(name: &str) -> Option<PathBuf> {
    let dirs = std::env::var("PATH").unwrap_or_default();
    // on Windows programs end in .exe
    let names: Vec<String> = if cfg!(windows) { vec![format!("{name}.exe"), name.to_string()] } else { vec![name.to_string()] };
    std::env::split_paths(&dirs)
        .chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from))
        .flat_map(|d| names.iter().map(move |n| d.join(n)).collect::<Vec<_>>())
        .find(|p| p.is_file())
}

fn oda() -> Option<PathBuf> {
    let fixed = ["/Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter", "/usr/bin/ODAFileConverter", "/usr/local/bin/ODAFileConverter"].iter().map(PathBuf::from).find(|p| p.is_file());
    fixed.or_else(|| find("ODAFileConverter")).or_else(oda_windows)
}

/// The ODA File Converter on Windows installs to a versioned folder:
/// C:\Program Files\ODA\ODAFileConverter 25.12.0\ODAFileConverter.exe
fn oda_windows() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let roots = ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|v| std::env::var_os(v)).map(|p| PathBuf::from(p).join("ODA"));
    let mut found: Vec<PathBuf> = roots
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flatten()
        .flatten()
        .map(|e| e.path().join("ODAFileConverter.exe"))
        .filter(|p| p.is_file())
        .collect();
    found.sort(); // the newest version sorts last
    found.pop()
}

/// Which converter is available (for the app to say so).
pub fn converter() -> Option<&'static str> {
    if find("dwg2dxf").is_some() {
        Some("LibreDWG")
    } else if oda().is_some() {
        Some("ODA File Converter")
    } else {
        None
    }
}

fn run(c: Command) -> Result<()> {
    let out = crate::helper::run(c, crate::helper::DWG).map_err(|e| anyhow::anyhow!("the DWG converter: {}", crate::helper::why(&e)))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("the DWG converter failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message"));
    }
    Ok(())
}

/// Convert a DWG to DXF in `tmp`; returns the DXF path.
pub fn to_dxf(dwg: &Path, tmp: &Path) -> Result<PathBuf> {
    if !dwg.is_file() {
        bail!("no file at {}", dwg.display());
    }
    let out = tmp.join("drawing.dxf");
    if let Some(bin) = find("dwg2dxf") {
        let mut c = Command::new(bin);
        c.arg("-y").arg("-o").arg(crate::safety::path_arg(&out)).arg(crate::safety::path_arg(dwg));
        run(c)?;
    } else if let Some(bin) = oda() {
        // ODA converts folders: copy the one file into its own input folder
        let input = tmp.join("in");
        std::fs::create_dir_all(&input)?;
        std::fs::copy(dwg, input.join("drawing.dwg"))?;
        let mut c = Command::new(bin);
        c.arg(crate::safety::path_arg(&input)).arg(crate::safety::path_arg(tmp)).args(["ACAD2018", "DXF", "0", "1", "*.DWG"]);
        run(c)?;
    } else {
        bail!("{HELP}");
    }
    if !out.is_file() {
        bail!("the DWG converter didn't produce a drawing (is the file a DWG?)");
    }
    Ok(out)
}

/// Convert our DXF to a DWG at `dwg`. The converter writes beside the DXF (a folder of our own), and the drawing is
/// then written to `dwg` as every other export is (`write_output`: not through a link for an agent or a plugin).
pub fn from_dxf(dxf: &Path, dwg: &Path) -> Result<()> {
    if let Some(bin) = find("dxf2dwg") {
        let made = dxf.with_extension("dwg");
        let mut c = Command::new(bin);
        c.arg("-y").arg("-o").arg(crate::safety::path_arg(&made)).arg(crate::safety::path_arg(dxf));
        run(c)?;
        crate::safety::write_output(dwg, std::fs::read(&made).context("the DWG converter didn't produce a drawing")?)?;
    } else if let Some(bin) = oda() {
        let dir = dxf.parent().unwrap_or(Path::new("."));
        let out = dir.join("out");
        std::fs::create_dir_all(&out)?;
        let mut c = Command::new(bin);
        c.arg(crate::safety::path_arg(dir)).arg(crate::safety::path_arg(&out)).args(["ACAD2018", "DWG", "0", "1", "*.DXF"]);
        run(c)?;
        let made = out.join(dxf.with_extension("dwg").file_name().unwrap());
        crate::safety::write_output(dwg, std::fs::read(&made).context("the DWG converter didn't produce a drawing")?)?;
    } else {
        bail!("{HELP}");
    }
    if !dwg.is_file() {
        bail!("the DWG converter didn't write {}", dwg.display());
    }
    Ok(())
}
